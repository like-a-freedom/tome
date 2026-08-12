//! Application service for updating installed skills.
//!
//! The service owns update orchestration while command modules remain responsible
//! for argument handling and presentation.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::{
    manifest::Manifest,
    repo_detect::{self, RepoType},
    spec,
};
use crate::fs;
const SHORT_COMMIT_LEN: usize = 7;

/// Progress phases emitted while an update source is processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressPhase {
    Downloading,
    Cloning,
    Inspecting,
    Checking,
    Copying,
    Saving,
}

/// Aggregate outcome emitted when an update source finishes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RepoOutcome {
    pub updated: usize,
    pub current: usize,
    pub failed: usize,
    pub removed: usize,
}

impl RepoOutcome {
    pub fn new(updated: usize, current: usize, failed: usize, removed: usize) -> Self {
        Self {
            updated,
            current,
            failed,
            removed,
        }
    }
}

/// Reports update progress without coupling the service to a presentation implementation.
pub trait UpdateReporter {
    fn start(&mut self, total_skills: usize, total_sources: usize);
    fn repo_started(
        &mut self,
        source_idx: usize,
        source_count: usize,
        label: &str,
        skill_count: usize,
    );
    fn phase(&mut self, phase: ProgressPhase);
    fn skill_checked(&mut self);
    fn repo_finished(&mut self, label: &str, outcome: RepoOutcome);
    fn is_live(&self) -> bool;
}

/// Source kind used for verbose update output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateSource {
    Git,
    File,
}

/// Metadata for an updated or skipped skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub name: String,
    pub source: UpdateSource,
    pub old_commit: String,
    pub new_commit: String,
    pub version: String,
}

/// Failure details for verbose output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFailure {
    pub name: String,
    pub reason: String,
}

/// Result of one update run.
#[derive(Debug, Default)]
pub struct UpdateResult {
    pub updated: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
    pub removed: Vec<String>,
    pub stale: Vec<String>,
    pub updated_details: Vec<UpdateInfo>,
    pub skipped_details: Vec<UpdateInfo>,
    pub failures: Vec<UpdateFailure>,
}

impl UpdateResult {
    /// Return whether no skill was processed.
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    /// Return the number of processed skills.
    pub fn total(&self) -> usize {
        self.updated.len() + self.skipped.len() + self.failed.len() + self.removed.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FileUpdateResult {
    Updated(UpdateInfo),
    Skipped(UpdateInfo),
    Failed(String),
}

#[derive(Debug, Clone, Copy)]
struct SourcePosition {
    index: usize,
    total: usize,
}

/// Application service for updating skills from their recorded sources.
pub struct UpdateService {
    skills_dir: PathBuf,
    manifest_path: PathBuf,
}

impl UpdateService {
    /// Create a service with an explicit manifest path.
    pub fn with_manifest_path(skills_dir: PathBuf, manifest_path: PathBuf) -> Self {
        Self {
            skills_dir,
            manifest_path,
        }
    }

    /// Update all skills or the skills matching `specific_name`.
    ///
    /// Git skills sharing one remote are cloned once. File URL skills are
    /// downloaded individually. The manifest is saved once after all sources
    /// have been processed, so an update cannot persist a partially mutated
    /// entry between skills in the same run.
    pub fn run(
        &self,
        specific_name: Option<&str>,
        sparse: bool,
        reporter: &mut impl UpdateReporter,
    ) -> Result<UpdateResult> {
        let manifest_path = self.manifest_path();
        let mut manifest = Manifest::load(manifest_path)?;
        let names = self.selected_names(&manifest, specific_name)?;
        let mut result = UpdateResult {
            stale: manifest.find_stale(&self.skills_dir),
            ..UpdateResult::default()
        };

        if names.is_empty() {
            return Ok(result);
        }

        let total_skills = names.len();
        let (file_names, git_names): (Vec<String>, Vec<String>) =
            names.into_iter().partition(|name| {
                manifest
                    .get_skill(name)
                    .is_some_and(|entry| entry.commit.starts_with("file:"))
            });

        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for name in git_names {
            if let Some(entry) = manifest.get_skill(&name) {
                groups.entry(entry.remote.clone()).or_default().push(name);
            }
        }

        let mut group_list: Vec<(String, Vec<String>)> = groups.into_iter().collect();
        group_list.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
        let source_count = file_names.len() + group_list.len();
        reporter.start(total_skills, source_count);

        for (index, name) in file_names.iter().enumerate() {
            match self.update_file_skill(&mut manifest, name, index + 1, source_count, reporter) {
                FileUpdateResult::Updated(info) => {
                    result.updated.push(name.clone());
                    result.updated_details.push(info);
                }
                FileUpdateResult::Skipped(info) => {
                    result.skipped.push(name.clone());
                    result.skipped_details.push(info);
                }
                FileUpdateResult::Failed(reason) => {
                    result.failed.push(name.clone());
                    result.failures.push(UpdateFailure {
                        name: name.clone(),
                        reason,
                    });
                }
            }
        }

        for (index, (remote, skill_names)) in group_list.iter().enumerate() {
            let group_result = self.update_git_group(
                &mut manifest,
                remote,
                skill_names,
                sparse,
                SourcePosition {
                    index: file_names.len() + index + 1,
                    total: source_count,
                },
                reporter,
            )?;
            result.updated.extend(group_result.updated);
            result.skipped.extend(group_result.skipped);
            result.failed.extend(group_result.failed);
            result.removed.extend(group_result.removed);
            result.updated_details.extend(group_result.updated_details);
            result.skipped_details.extend(group_result.skipped_details);
            result.failures.extend(group_result.failures);
        }

        result.stale = manifest.find_stale(&self.skills_dir);
        reporter.phase(ProgressPhase::Saving);
        manifest.save(manifest_path)?;
        Ok(result)
    }

    fn selected_names(
        &self,
        manifest: &Manifest,
        specific_name: Option<&str>,
    ) -> Result<Vec<String>> {
        match specific_name {
            Some(name) => {
                let names = manifest.resolve_names(name);
                if names.is_empty() {
                    anyhow::bail!("Skill '{}' not found in manifest.", name);
                }
                Ok(names)
            }
            None => Ok(manifest.skill_names()),
        }
    }

    fn update_file_skill(
        &self,
        manifest: &mut Manifest,
        name: &str,
        source_idx: usize,
        source_count: usize,
        reporter: &mut impl UpdateReporter,
    ) -> FileUpdateResult {
        let (url, old_commit, version) = match manifest.get_skill(name) {
            Some(entry) => (
                entry.remote.clone(),
                entry.commit.clone(),
                entry.version.clone(),
            ),
            None => return FileUpdateResult::Failed("skill missing from manifest".to_string()),
        };
        reporter.repo_started(source_idx, source_count, name, 1);
        reporter.phase(ProgressPhase::Downloading);

        let downloaded = match crate::core::install::download_skill_file(&url) {
            Ok(downloaded) => downloaded,
            Err(error) => {
                reporter.skill_checked();
                reporter.repo_finished(name, RepoOutcome::new(0, 0, 1, 0));
                return FileUpdateResult::Failed(format!("download failed: {error}"));
            }
        };

        let skill_dir = self.skills_dir.join(name);
        let existing_path = skill_dir.join("SKILL.md");
        if existing_path.exists()
            && std::fs::read(&existing_path).is_ok_and(|existing| existing == downloaded.content)
        {
            reporter.skill_checked();
            reporter.repo_finished(name, RepoOutcome::new(0, 1, 0, 0));
            return FileUpdateResult::Skipped(UpdateInfo {
                name: name.to_string(),
                source: UpdateSource::File,
                old_commit: old_commit.clone(),
                new_commit: old_commit,
                version,
            });
        }

        let frontmatter = match spec::validate_skill(&downloaded.path) {
            Ok(frontmatter) => frontmatter,
            Err(error) => {
                reporter.skill_checked();
                reporter.repo_finished(name, RepoOutcome::new(0, 0, 1, 0));
                return FileUpdateResult::Failed(format!("invalid SKILL.md: {error}"));
            }
        };

        reporter.phase(ProgressPhase::Copying);
        if let Err(error) = fs::copy_skill(&downloaded.path, &skill_dir) {
            reporter.skill_checked();
            reporter.repo_finished(name, RepoOutcome::new(0, 0, 1, 0));
            return FileUpdateResult::Failed(format!("copy failed: {error}"));
        }
        let version = frontmatter.version.unwrap_or_default();
        manifest.update_skill(name, "file:", &version, "", frontmatter.description);
        reporter.skill_checked();
        reporter.repo_finished(name, RepoOutcome::new(1, 0, 0, 0));

        FileUpdateResult::Updated(UpdateInfo {
            name: name.to_string(),
            source: UpdateSource::File,
            old_commit,
            new_commit: "file:".to_string(),
            version,
        })
    }

    fn update_git_group(
        &self,
        manifest: &mut Manifest,
        remote: &str,
        skill_names: &[String],
        sparse: bool,
        position: SourcePosition,
        reporter: &mut impl UpdateReporter,
    ) -> Result<UpdateResult> {
        let mut result = UpdateResult::default();
        let label = crate::core::parse_source::repo_label(remote).to_string();
        reporter.repo_started(position.index, position.total, &label, skill_names.len());
        reporter.phase(ProgressPhase::Cloning);
        let temp_dir = fs::create_temp_dir()?;
        let git_output = if reporter.is_live() {
            crate::core::git::GitOutput::Quiet
        } else {
            crate::core::git::GitOutput::Progress
        };

        if let Err(error) =
            crate::core::git::clone_repo_or_sparse(remote, temp_dir.path(), sparse, git_output)
        {
            for name in skill_names {
                record_failure(&mut result, name, format!("clone failed: {error}"));
                reporter.skill_checked();
            }
            reporter.repo_finished(&label, RepoOutcome::new(0, 0, skill_names.len(), 0));
            return Ok(result);
        }

        reporter.phase(ProgressPhase::Inspecting);
        let repo_path = temp_dir.path();
        let new_commit = match crate::core::git::get_commit_hash(repo_path) {
            Ok(commit) => commit,
            Err(error) => {
                for name in skill_names {
                    record_failure(&mut result, name, format!("commit lookup failed: {error}"));
                    reporter.skill_checked();
                }
                reporter.repo_finished(&label, RepoOutcome::new(0, 0, skill_names.len(), 0));
                return Ok(result);
            }
        };
        let branch = crate::core::git::get_branch(repo_path)
            .unwrap_or_else(|_| crate::DEFAULT_BRANCH.to_string());

        for name in skill_names {
            reporter.phase(ProgressPhase::Checking);
            let entry = match manifest.get_skill(name) {
                Some(entry) => entry.clone(),
                None => {
                    reporter.skill_checked();
                    continue;
                }
            };
            let source_path = match skill_repo_path(repo_path, &entry.path) {
                Ok(path) => path,
                Err(error) => {
                    record_failure(&mut result, name, format!("invalid skill path: {error}"));
                    reporter.skill_checked();
                    continue;
                }
            };
            let local_path = self.skills_dir.join(name);

            if !source_path.join("SKILL.md").exists() {
                if let Err(error) = fs::remove_skill_dir(&local_path) {
                    record_failure(&mut result, name, format!("remove failed: {error}"));
                    reporter.skill_checked();
                    continue;
                }
                manifest.remove_skill(name);
                result.removed.push(name.clone());
                reporter.skill_checked();
                continue;
            }

            let frontmatter = match spec::validate_skill(&source_path) {
                Ok(frontmatter) => frontmatter,
                Err(error) => {
                    record_failure(&mut result, name, format!("invalid SKILL.md: {error}"));
                    reporter.skill_checked();
                    continue;
                }
            };

            let local_skill_missing = !local_path.join("SKILL.md").exists();
            if commits_match(&entry.commit, &new_commit) && !local_skill_missing {
                result.skipped.push(name.clone());
                result.skipped_details.push(UpdateInfo {
                    name: name.clone(),
                    source: UpdateSource::Git,
                    old_commit: entry.commit.clone(),
                    new_commit: new_commit.clone(),
                    version: entry.version.clone(),
                });
                reporter.skill_checked();
                continue;
            }

            reporter.phase(ProgressPhase::Copying);
            if let Err(error) = fs::copy_skill(&source_path, &local_path) {
                record_failure(&mut result, name, format!("copy failed: {error}"));
                reporter.skill_checked();
                continue;
            }

            let version = frontmatter.version.unwrap_or_default();
            manifest.update_skill(
                name,
                &new_commit,
                &version,
                &branch,
                frontmatter.description,
            );
            result.updated.push(name.clone());
            result.updated_details.push(UpdateInfo {
                name: name.clone(),
                source: UpdateSource::Git,
                old_commit: entry.commit,
                new_commit: new_commit.clone(),
                version,
            });
            reporter.skill_checked();
        }

        reporter.repo_finished(
            &label,
            RepoOutcome::new(
                result.updated.len(),
                result.skipped.len(),
                result.failed.len(),
                result.removed.len(),
            ),
        );
        Ok(result)
    }

    fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }
}

fn record_failure(result: &mut UpdateResult, name: &str, reason: String) {
    result.failed.push(name.to_string());
    result.failures.push(UpdateFailure {
        name: name.to_string(),
        reason,
    });
}

fn skill_repo_path(clone_dir: &Path, entry_path: &str) -> Result<PathBuf> {
    if !entry_path.is_empty() {
        return crate::fs::resolve_path_within(clone_dir, entry_path);
    }

    let candidate = match repo_detect::detect_repo_type(clone_dir) {
        Ok(RepoType::SingleSkillAtRoot) => clone_dir.to_path_buf(),
        Ok(RepoType::SingleSkillInDir { path, .. }) => path,
        _ => clone_dir.to_path_buf(),
    };
    crate::fs::path_within(clone_dir, &candidate)
}

fn commits_match(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }

    if left.len() > SHORT_COMMIT_LEN && right.len() > SHORT_COMMIT_LEN {
        return false;
    }

    left.starts_with(right) || right.starts_with(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::git;
    use crate::core::manifest::{Manifest, SkillEntry};

    use crate::testutil;
    use std::fs;
    use tempfile::TempDir;

    fn make_entry(name: &str, commit: &str, remote: &str) -> SkillEntry {
        let frontmatter = testutil::make_fm(name);
        SkillEntry::new(&format!("owner/{name}"), "", &frontmatter, commit, remote)
    }

    #[derive(Default)]
    struct RecordingReporter {
        phases: Vec<ProgressPhase>,
    }

    impl UpdateReporter for RecordingReporter {
        fn start(&mut self, _total_skills: usize, _total_sources: usize) {}

        fn repo_started(
            &mut self,
            _source_idx: usize,
            _source_count: usize,
            _label: &str,
            _skill_count: usize,
        ) {
        }

        fn phase(&mut self, phase: ProgressPhase) {
            self.phases.push(phase);
        }

        fn skill_checked(&mut self) {}

        fn repo_finished(&mut self, _label: &str, _outcome: RepoOutcome) {}

        fn is_live(&self) -> bool {
            false
        }
    }

    #[test]
    fn update_reports_saving_before_persisting() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let local = skills_dir.join("file-skill");
        fs::create_dir_all(&local).unwrap();
        let content = "---\nname: file-skill\ndescription: unchanged\n---\n";
        fs::write(local.join("SKILL.md"), content).unwrap();
        let source = temp.path().join("source-SKILL.md");
        fs::write(&source, content).unwrap();
        let source_url = format!("file://{}", source.display());
        let manifest_path = skills_dir.join(".skills.json");
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "file-skill",
            SkillEntry::new(
                "source",
                "",
                &testutil::make_fm("file-skill"),
                "file:",
                &source_url,
            ),
        );
        manifest.save(&manifest_path).unwrap();

        let service = UpdateService::with_manifest_path(skills_dir, manifest_path);
        let mut reporter = RecordingReporter::default();
        service.run(None, false, &mut reporter).unwrap();

        assert!(reporter.phases.contains(&ProgressPhase::Downloading));
        assert_eq!(reporter.phases.last(), Some(&ProgressPhase::Saving));
    }

    #[test]
    fn update_group_copies_new_repository_content_and_updates_manifest() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let remote = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: changed\ndescription: new\n---\nnew body\n",
        );
        let clone = TempDir::new().unwrap();
        git::clone_repo_quiet(&remote, clone.path()).unwrap();
        let commit = git::get_commit_hash(clone.path()).unwrap();

        let local = skills_dir.join("changed");
        fs::create_dir_all(&local).unwrap();
        fs::write(
            local.join("SKILL.md"),
            "---\nname: changed\ndescription: old\n---\nold body\n",
        )
        .unwrap();

        let mut manifest = Manifest::new();
        manifest.add_skill("changed", make_entry("changed", "0000000", &remote));

        let service =
            UpdateService::with_manifest_path(skills_dir.clone(), skills_dir.join(".skills.json"));
        let mut reporter = crate::progress::UpdateProgress::new(false);
        let result = service
            .update_git_group(
                &mut manifest,
                &remote,
                &["changed".to_string()],
                false,
                SourcePosition { index: 1, total: 1 },
                &mut reporter,
            )
            .unwrap();

        assert_eq!(result.updated, vec!["changed"]);
        assert_eq!(
            fs::read_to_string(local.join("SKILL.md")).unwrap(),
            "---\nname: changed\ndescription: new\n---\nnew body\n"
        );
        assert_eq!(manifest.get_skill("changed").unwrap().commit, commit);
    }

    #[test]
    fn update_group_restores_missing_local_entries() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let remote = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: present\ndescription: present\n---\n",
        );
        let mut manifest = Manifest::new();
        manifest.add_skill("missing", make_entry("missing", "0000000", &remote));

        let service =
            UpdateService::with_manifest_path(skills_dir.clone(), skills_dir.join(".skills.json"));
        let mut reporter = crate::progress::UpdateProgress::new(false);
        let result = service
            .update_git_group(
                &mut manifest,
                &remote,
                &["missing".to_string()],
                false,
                SourcePosition { index: 1, total: 1 },
                &mut reporter,
            )
            .unwrap();

        assert_eq!(result.updated, vec!["missing"]);
        assert!(manifest.is_tracked("missing"));
        assert!(skills_dir.join("missing/SKILL.md").exists());
    }

    #[test]
    fn update_run_restores_missing_local_skill() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let remote = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: restored\ndescription: restored\n---\nbody\n",
        );
        let current_commit = git::get_commit_hash(&temp.path().join("repo")).unwrap();
        let mut manifest = Manifest::new();
        manifest.add_skill("restored", make_entry("restored", &current_commit, &remote));
        let manifest_path = temp.path().join("custom-manifest.json");
        manifest.save(&manifest_path).unwrap();

        let service = UpdateService::with_manifest_path(skills_dir.clone(), manifest_path);
        let mut reporter = crate::progress::UpdateProgress::new(false);
        let result = service.run(None, true, &mut reporter).unwrap();

        assert_eq!(result.updated, vec!["restored"]);
        assert_eq!(result.failed, Vec::<String>::new());
        assert!(skills_dir.join("restored/SKILL.md").exists());
    }

    #[test]
    fn update_run_removes_skill_missing_from_source() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("deleted")).unwrap();
        fs::write(
            skills_dir.join("deleted/SKILL.md"),
            "---\nname: deleted\ndescription: old\n---\n",
        )
        .unwrap();
        let remote = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: deleted\ndescription: old\n---\n",
        );
        let remote_path = temp.path().join("repo");
        fs::remove_file(remote_path.join("SKILL.md")).unwrap();
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&remote_path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "remove skill"])
            .current_dir(&remote_path)
            .output()
            .unwrap();

        let mut manifest = Manifest::new();
        manifest.add_skill("deleted", make_entry("deleted", "0000000", &remote));
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        let service = UpdateService::with_manifest_path(skills_dir.clone(), manifest_path.clone());
        let mut reporter = crate::progress::UpdateProgress::new(false);
        let result = service.run(None, true, &mut reporter).unwrap();
        let loaded = Manifest::load(&manifest_path).unwrap();

        assert_eq!(result.removed, vec!["deleted"]);
        assert!(!skills_dir.join("deleted").exists());
        assert!(!loaded.is_tracked("deleted"));
    }

    #[test]
    fn commits_match_accepts_full_and_short_hashes() {
        assert!(commits_match("abcdef1", "abcdef123456"));
        assert!(commits_match("abcdef1", "abcdef1"));
        assert!(!commits_match("abcdef123456", "abcdef999999"));
        assert!(!commits_match("abcdef1", "1234567"));
    }
}
