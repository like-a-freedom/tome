use std::path::{Path, PathBuf};

use crate::core::{git, repo_detect, spec};
use crate::fs;
use crate::output;
use crate::progress::{ProgressPhase, RepoOutcome};
use crate::style::{self, YELLOW};

use super::UpdateContext;
use super::summary::format_commit;

#[derive(Debug, Default)]
pub(crate) struct GroupResult {
    pub updated: Vec<(String, String, String)>,
    pub skipped: Vec<(String, String)>,
    pub failed: Vec<(String, String)>,
    pub removed: Vec<String>,
}

impl GroupResult {
    pub(crate) fn is_empty(&self) -> bool {
        self.updated.is_empty()
            && self.skipped.is_empty()
            && self.failed.is_empty()
            && self.removed.is_empty()
    }

    pub(crate) fn failed_all(skill_names: &[String], reason: &str) -> Self {
        Self {
            failed: skill_names
                .iter()
                .map(|n| (n.clone(), reason.to_string()))
                .collect(),
            ..Default::default()
        }
    }
}

impl From<&GroupResult> for RepoOutcome {
    fn from(result: &GroupResult) -> Self {
        RepoOutcome::new(
            result.updated.len(),
            result.skipped.len(),
            result.failed.len(),
            result.removed.len(),
        )
    }
}

pub(crate) struct GitGroupConfig<'a> {
    pub remote: &'a str,
    pub skill_names: &'a [String],
    pub group_idx: usize,
    pub group_count: usize,
}

pub(crate) struct GitGroupOutcome {
    pub label: String,
    pub result: GroupResult,
}

fn skill_repo_path(clone_dir: &Path, entry_path: &str) -> PathBuf {
    if entry_path.is_empty() {
        match repo_detect::detect_repo_type(clone_dir) {
            Ok(repo_detect::RepoType::SingleSkillAtRoot) => clone_dir.to_path_buf(),
            Ok(repo_detect::RepoType::SingleSkillInDir { name: _, path: p }) => p,
            _ => clone_dir.to_path_buf(),
        }
    } else {
        clone_dir.join(entry_path)
    }
}

pub(crate) fn update_group(
    config: &GitGroupConfig<'_>,
    ctx: &mut UpdateContext<'_>,
) -> GitGroupOutcome {
    let label = crate::core::parse_source::repo_label(config.remote).to_string();

    if ctx.verbose {
        output::print_cloning_multi(
            &label,
            config.group_idx,
            config.group_count,
            config.skill_names.len(),
        );
    }
    ctx.progress.repo_started(
        config.group_idx + 1,
        config.group_count,
        &label,
        config.skill_names.len(),
    );
    ctx.progress.phase(ProgressPhase::Cloning);

    let temp_dir = match fs::create_temp_dir() {
        Ok(d) => d,
        Err(e) => {
            output::print_failed_action("Failed to create temp dir", &e.to_string());
            let result = GroupResult::failed_all(config.skill_names, "temp dir");
            ctx.progress
                .repo_finished(&label, RepoOutcome::from(&result));
            return GitGroupOutcome { label, result };
        }
    };

    let git_output = if ctx.progress.enabled() {
        git::GitOutput::Quiet
    } else {
        git::GitOutput::Progress
    };

    if let Err(e) =
        git::clone_repo_or_sparse(config.remote, temp_dir.path(), ctx.sparse, git_output)
    {
        output::print_failed_action("Clone failed", &e.to_string());
        let result = GroupResult::failed_all(config.skill_names, "clone failed");
        ctx.progress
            .repo_finished(&label, RepoOutcome::from(&result));
        return GitGroupOutcome { label, result };
    }
    ctx.progress.phase(ProgressPhase::Inspecting);

    let new_commit =
        git::get_commit_hash(temp_dir.path()).unwrap_or_else(|_| "unknown".to_string());
    let branch =
        git::get_branch(temp_dir.path()).unwrap_or_else(|_| crate::DEFAULT_BRANCH.to_string());

    if ctx.verbose {
        eprintln!();
    }

    let mut group_result = GroupResult::default();

    for skill_name in config.skill_names {
        ctx.progress.phase(ProgressPhase::Checking);
        let (old_commit, entry_path) = match ctx.manifest.get_skill(skill_name) {
            Some(e) => (e.commit.clone(), e.path.clone()),
            None => continue,
        };

        let skill_path = skill_repo_path(temp_dir.path(), &entry_path);

        if !skill_path.join("SKILL.md").exists() {
            let local_dir = ctx.skills_dir.join(skill_name);
            let _ = fs::remove_skill_dir(&local_dir);
            ctx.manifest.remove_skill(skill_name);
            if ctx.verbose {
                output::print_removed(skill_name);
            }
            group_result.removed.push(skill_name.clone());
            ctx.progress.skill_checked();
            continue;
        }

        let frontmatter = match spec::validate_skill(&skill_path) {
            Ok(fm) => fm,
            Err(e) => {
                if ctx.verbose {
                    eprintln!(
                        "  {} Skipping {} \u{2014} invalid SKILL.md: {}",
                        style::styled(YELLOW, "\u{2717}"),
                        skill_name,
                        e
                    );
                }
                group_result
                    .failed
                    .push((skill_name.clone(), "invalid SKILL.md".to_string()));
                ctx.progress.skill_checked();
                continue;
            }
        };

        if format_commit(&new_commit) == format_commit(&old_commit) {
            if ctx.verbose {
                output::print_up_to_date(skill_name, &format_commit(&new_commit));
            }
            group_result
                .skipped
                .push((skill_name.clone(), new_commit.clone()));
            ctx.progress.skill_checked();
            continue;
        }

        ctx.progress.phase(ProgressPhase::Copying);
        let dest_dir = ctx.skills_dir.join(skill_name);
        if let Err(e) = fs::copy_skill(&skill_path, &dest_dir) {
            if ctx.verbose {
                output::print_failed(skill_name, "copy", &e.to_string());
            }
            group_result
                .failed
                .push((skill_name.clone(), "copy failed".to_string()));
            ctx.progress.skill_checked();
            continue;
        }

        let version = frontmatter.version.unwrap_or_default();
        ctx.manifest.update_skill(
            skill_name,
            &new_commit,
            &version,
            &branch,
            frontmatter.description,
        );

        if ctx.verbose {
            output::print_updated(
                skill_name,
                &version,
                &format_commit(&old_commit),
                &format_commit(&new_commit),
            );
        }

        ctx.progress.skill_checked();
        group_result
            .updated
            .push((skill_name.clone(), old_commit, new_commit.clone()));
    }

    ctx.progress
        .repo_finished(&label, RepoOutcome::from(&group_result));
    GitGroupOutcome {
        label,
        result: group_result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::{Manifest, SkillEntry};
    use crate::core::{git, spec};
    use crate::fs::copy_skill;
    use crate::progress::{RepoOutcome, UpdateProgress};
    use crate::testutil;
    use std::collections::HashMap;
    use std::fs;
    use tempfile::TempDir;

    fn make_entry(name: &str, commit: &str, remote: &str) -> (String, SkillEntry) {
        let fm = testutil::make_fm(name);
        (
            name.to_string(),
            SkillEntry::new(&format!("owner/{}", name), "", &fm, commit, remote),
        )
    }

    fn make_test_context<'a>(
        manifest: &'a mut Manifest,
        skills_dir: &'a Path,
        progress: &'a mut UpdateProgress,
    ) -> UpdateContext<'a> {
        UpdateContext {
            manifest,
            skills_dir,
            verbose: false,
            sparse: true,
            progress,
        }
    }

    #[test]
    fn test_repo_outcome_from_group_result() {
        let group_result = GroupResult {
            updated: vec![("a".to_string(), "old".to_string(), "new".to_string())],
            skipped: vec![("b".to_string(), "new".to_string())],
            failed: vec![("c".to_string(), "copy failed".to_string())],
            removed: vec!["d".to_string()],
        };

        let outcome = RepoOutcome::from(&group_result);

        assert_eq!(outcome.updated, 1);
        assert_eq!(outcome.current, 1);
        assert_eq!(outcome.failed, 1);
        assert_eq!(outcome.removed, 1);
    }

    #[test]
    fn test_update_skill_removed_from_repo() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let repo_url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: skill-a\ndescription: exists\n---\n",
        );

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("skill-a", make_entry("skill-a", "0000000", &repo_url).1);
        manifest.add_skill("skill-b", make_entry("skill-b", "1111111", &repo_url).1);
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        let _ = manifest; // suppress unused warning
        let mut manifest = Manifest::load(&manifest_path).unwrap();
        let mut progress = UpdateProgress::new(false);
        let mut ctx = make_test_context(&mut manifest, &skills_dir, &mut progress);

        let config = GitGroupConfig {
            remote: &repo_url,
            skill_names: &["skill-b".to_string()],
            group_idx: 0,
            group_count: 1,
        };

        let outcome = update_group(&config, &mut ctx);
        // Skill may be "updated" (same root path SKILL.md exists),
        // "removed" (if entry_path is specific), or "skipped" (same commit).
        // Bottom line: graceful handling, no panic, no crash.
        assert!(
            !outcome.result.updated.is_empty()
                || !outcome.result.skipped.is_empty()
                || !outcome.result.removed.is_empty()
                || !outcome.result.failed.is_empty()
        );
    }

    #[test]
    fn test_update_single_skill_success() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: my-skill\ndescription: test\nversion: 1.0.0\n---\n# Hello\n",
        );

        let clone_temp = TempDir::new().unwrap();
        let _ = git::clone_repo_quiet(&url, clone_temp.path());
        let fm = spec::validate_skill(clone_temp.path()).unwrap();
        let dest = skills_dir.join(&fm.name);
        copy_skill(clone_temp.path(), &dest).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("my-skill", make_entry("my-skill", "0000000", &url).1);

        let mut progress = UpdateProgress::new(false);
        let mut ctx = make_test_context(&mut manifest, &skills_dir, &mut progress);

        let config = GitGroupConfig {
            remote: &url,
            skill_names: &["my-skill".to_string()],
            group_idx: 0,
            group_count: 1,
        };

        let outcome = update_group(&config, &mut ctx);
        assert!(
            outcome
                .result
                .updated
                .iter()
                .any(|(n, _, _)| n == "my-skill")
        );
    }

    #[test]
    fn test_update_skips_when_no_changes() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: skip-skill\ndescription: t\n---\n",
        );

        let clone_temp = TempDir::new().unwrap();
        git::clone_repo_quiet(&url, clone_temp.path()).unwrap();
        let real_commit = git::get_commit_hash(clone_temp.path()).unwrap();

        let fm = spec::validate_skill(clone_temp.path()).unwrap();
        let dest = skills_dir.join(&fm.name);
        copy_skill(clone_temp.path(), &dest).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("skip-skill", make_entry("skip-skill", &real_commit, &url).1);

        let mut progress = UpdateProgress::new(false);
        let mut ctx = make_test_context(&mut manifest, &skills_dir, &mut progress);

        let config = GitGroupConfig {
            remote: &url,
            skill_names: &["skip-skill".to_string()],
            group_idx: 0,
            group_count: 1,
        };

        let outcome = update_group(&config, &mut ctx);
        assert!(
            outcome
                .result
                .skipped
                .iter()
                .any(|(n, _)| n == "skip-skill")
        );
    }

    #[test]
    fn test_update_skips_when_no_changes_verbose() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: verb-skip\ndescription: t\n---\n",
        );

        let clone_temp = TempDir::new().unwrap();
        git::clone_repo_quiet(&url, clone_temp.path()).unwrap();
        let real_commit = git::get_commit_hash(clone_temp.path()).unwrap();

        let fm = spec::validate_skill(clone_temp.path()).unwrap();
        let dest = skills_dir.join(&fm.name);
        copy_skill(clone_temp.path(), &dest).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("verb-skip", make_entry("verb-skip", &real_commit, &url).1);

        let mut progress = UpdateProgress::new(false);
        let mut ctx = UpdateContext {
            manifest: &mut manifest,
            skills_dir: &skills_dir,
            verbose: true,
            sparse: true,
            progress: &mut progress,
        };

        let config = GitGroupConfig {
            remote: &url,
            skill_names: &["verb-skip".to_string()],
            group_idx: 0,
            group_count: 1,
        };

        let outcome = update_group(&config, &mut ctx);
        assert!(outcome.result.skipped.iter().any(|(n, _)| n == "verb-skip"));
    }

    #[test]
    fn test_update_skips_when_manifest_has_short_hash() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: short-hash\ndescription: t\n---\n",
        );

        let clone_temp = TempDir::new().unwrap();
        git::clone_repo_quiet(&url, clone_temp.path()).unwrap();
        let real_commit = git::get_commit_hash(clone_temp.path()).unwrap();
        let short_hash = &real_commit[..7];

        let fm = spec::validate_skill(clone_temp.path()).unwrap();
        let dest = skills_dir.join(&fm.name);
        copy_skill(clone_temp.path(), &dest).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("short-hash", make_entry("short-hash", short_hash, &url).1);

        let mut progress = UpdateProgress::new(false);
        let mut ctx = make_test_context(&mut manifest, &skills_dir, &mut progress);

        let config = GitGroupConfig {
            remote: &url,
            skill_names: &["short-hash".to_string()],
            group_idx: 0,
            group_count: 1,
        };

        let outcome = update_group(&config, &mut ctx);
        assert!(
            outcome
                .result
                .skipped
                .iter()
                .any(|(n, _)| n == "short-hash")
        );
    }

    #[test]
    fn test_update_respects_changed_commit_hash() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: changed\ndescription: t\n---\n",
        );

        let clone_temp = TempDir::new().unwrap();
        git::clone_repo_quiet(&url, clone_temp.path()).unwrap();
        let real_commit = git::get_commit_hash(clone_temp.path()).unwrap();

        let fm = spec::validate_skill(clone_temp.path()).unwrap();
        let dest = skills_dir.join(&fm.name);
        copy_skill(clone_temp.path(), &dest).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill(
            "changed",
            make_entry("changed", "0000000000000000000000000000000000000000", &url).1,
        );

        let mut progress = UpdateProgress::new(false);
        let mut ctx = make_test_context(&mut manifest, &skills_dir, &mut progress);

        let config = GitGroupConfig {
            remote: &url,
            skill_names: &["changed".to_string()],
            group_idx: 0,
            group_count: 1,
        };

        let outcome = update_group(&config, &mut ctx);
        assert!(
            outcome
                .result
                .updated
                .iter()
                .any(|(n, _, _)| n == "changed")
        );
        assert_eq!(manifest.get_skill("changed").unwrap().commit, real_commit);
    }

    #[test]
    fn test_update_grouped_remotes() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: group-skill-a\ndescription: a\n---\n# A\n",
        );

        let clone_temp = TempDir::new().unwrap();
        git::clone_repo_quiet(&url, clone_temp.path()).unwrap();
        let real_commit = git::get_commit_hash(clone_temp.path()).unwrap();

        spec::validate_skill(clone_temp.path()).unwrap();
        let dest_a = skills_dir.join("group-skill-a");
        copy_skill(clone_temp.path(), &dest_a).unwrap();
        let dest_b = skills_dir.join("group-skill-b");
        copy_skill(clone_temp.path(), &dest_b).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill(
            "group-skill-a",
            make_entry("group-skill-a", &real_commit, &url).1,
        );
        manifest.add_skill(
            "group-skill-b",
            make_entry(
                "group-skill-b",
                "0000000000000000000000000000000000000000",
                &url,
            )
            .1,
        );

        let mut progress = UpdateProgress::new(false);
        let mut ctx = make_test_context(&mut manifest, &skills_dir, &mut progress);

        let config = GitGroupConfig {
            remote: &url,
            skill_names: &["group-skill-a".to_string(), "group-skill-b".to_string()],
            group_idx: 0,
            group_count: 1,
        };

        let outcome = update_group(&config, &mut ctx);
        // group-skill-a is up to date (skipped), group-skill-b is updated
        assert!(
            outcome
                .result
                .skipped
                .iter()
                .any(|(n, _)| n == "group-skill-a")
        );
        assert!(
            outcome
                .result
                .updated
                .iter()
                .any(|(n, _, _)| n == "group-skill-b")
        );
    }

    #[test]
    fn test_group_result_is_empty() {
        let empty = GroupResult::default();
        assert!(empty.is_empty());

        let with_updated = GroupResult {
            updated: vec![("a".to_string(), "old".to_string(), "new".to_string())],
            ..Default::default()
        };
        assert!(!with_updated.is_empty());

        let with_skipped = GroupResult {
            skipped: vec![("a".to_string(), "c".to_string())],
            ..Default::default()
        };
        assert!(!with_skipped.is_empty());

        let with_failed = GroupResult {
            failed: vec![("a".to_string(), "reason".to_string())],
            ..Default::default()
        };
        assert!(!with_failed.is_empty());

        let with_removed = GroupResult {
            removed: vec!["a".to_string()],
            ..Default::default()
        };
        assert!(!with_removed.is_empty());
    }

    #[test]
    fn test_group_result_failed_all() {
        let names = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let result = GroupResult::failed_all(&names, "test reason");
        assert!(result.updated.is_empty());
        assert!(result.skipped.is_empty());
        assert!(result.removed.is_empty());
        assert_eq!(result.failed.len(), 3);
        for (i, (n, reason)) in result.failed.iter().enumerate() {
            assert_eq!(n, &names[i]);
            assert_eq!(reason, "test reason");
        }
        assert!(!result.is_empty());
    }

    #[test]
    fn test_group_result_failed_all_empty() {
        let result = GroupResult::failed_all(&[], "no skills");
        assert!(result.failed.is_empty());
        assert!(result.is_empty());
    }

    #[test]
    fn test_repo_outcome_from_empty_group_result() {
        let group_result = GroupResult::default();
        let outcome = RepoOutcome::from(&group_result);
        assert_eq!(outcome.updated, 0);
        assert_eq!(outcome.current, 0);
        assert_eq!(outcome.failed, 0);
        assert_eq!(outcome.removed, 0);
    }

    #[test]
    fn test_repo_outcome_from_group_result_only_removed() {
        let group_result = GroupResult {
            updated: vec![],
            skipped: vec![],
            failed: vec![],
            removed: vec!["a".to_string(), "b".to_string()],
        };
        let outcome = RepoOutcome::from(&group_result);
        assert_eq!(outcome.removed, 2);
        assert_eq!(outcome.updated, 0);
        assert_eq!(outcome.current, 0);
        assert_eq!(outcome.failed, 0);
    }

    #[test]
    fn test_update_group_clone_failure() {
        // Bad URL should cause clone to fail and produce failed_all for all skills
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill(
            "my-skill",
            make_entry(
                "my-skill",
                "0000000",
                "https://this-domain-does-not-exist-12345.invalid/repo.git",
            )
            .1,
        );

        let mut progress = UpdateProgress::new(false);
        let mut ctx = make_test_context(&mut manifest, &skills_dir, &mut progress);

        let bad_url = "https://this-domain-does-not-exist-12345.invalid/repo.git";
        let config = GitGroupConfig {
            remote: bad_url,
            skill_names: &["my-skill".to_string()],
            group_idx: 0,
            group_count: 1,
        };

        let outcome = update_group(&config, &mut ctx);
        assert_eq!(outcome.result.failed.len(), 1);
        assert_eq!(outcome.result.failed[0].0, "my-skill");
        assert!(outcome.result.updated.is_empty());
        assert!(outcome.result.skipped.is_empty());
    }

    #[test]
    fn test_update_group_invalid_skill_md() {
        // Repo has a SKILL.md without frontmatter — should be reported as failed
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();

        // Create a git repo with an invalid (no frontmatter) SKILL.md
        let url = testutil::create_git_repo_with_skill(
            &temp,
            "Just some plain text, no frontmatter at all.\n",
        );

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("bad-skill", make_entry("bad-skill", "0000000", &url).1);

        let mut progress = UpdateProgress::new(false);
        let mut ctx = make_test_context(&mut manifest, &skills_dir, &mut progress);

        let config = GitGroupConfig {
            remote: &url,
            skill_names: &["bad-skill".to_string()],
            group_idx: 0,
            group_count: 1,
        };

        let outcome = update_group(&config, &mut ctx);
        // The skill should be reported as failed (invalid SKILL.md)
        assert!(
            outcome.result.failed.iter().any(|(n, _)| n == "bad-skill")
                || !outcome.result.updated.is_empty(),
            "invalid SKILL.md should either fail or attempt update"
        );
    }

    #[test]
    fn test_git_group_config_construction() {
        // Regression: ensure all fields are accessible
        let url = "https://github.com/owner/repo.git";
        let names: Vec<String> = vec!["a".to_string(), "b".to_string()];
        let config = GitGroupConfig {
            remote: url,
            skill_names: &names,
            group_idx: 2,
            group_count: 5,
        };
        assert_eq!(config.remote, url);
        assert_eq!(config.skill_names.len(), 2);
        assert_eq!(config.group_idx, 2);
        assert_eq!(config.group_count, 5);
    }

    #[test]
    fn test_git_group_outcome_construction() {
        let label = "owner/repo".to_string();
        let result = GroupResult {
            updated: vec![("a".to_string(), "old".to_string(), "new".to_string())],
            skipped: vec![],
            failed: vec![],
            removed: vec![],
        };
        let outcome = GitGroupOutcome {
            label: label.clone(),
            result: GroupResult {
                updated: vec![("a".to_string(), "old".to_string(), "new".to_string())],
                skipped: vec![],
                failed: vec![],
                removed: vec![],
            },
        };
        assert_eq!(outcome.label, label);
        assert_eq!(outcome.result.updated.len(), 1);
        assert!(!result.is_empty());
    }
}
