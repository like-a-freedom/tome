//! SkillService — manages skill lifecycle (install, remove).
//!
//! This is the Application Service Layer: business logic isolated from I/O.
//! Commands call service methods, and services use core modules for actual operations.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::core::{parse_source, repo_detect, spec};

/// Injection point replacing the interactive TUI multi-select.
pub type SkillPicker = dyn FnMut(&[repo_detect::SkillInfo]) -> Result<Vec<repo_detect::SkillInfo>>;

// ── SkillService — main service ────────────────────────────

/// Service for managing skill lifecycle (install, remove).
pub struct SkillService {
    config: crate::services::ServiceConfig,
}

impl SkillService {
    /// Create a service with an explicit manifest path.
    pub fn with_manifest_path(skills_dir: PathBuf, manifest_path: PathBuf) -> Self {
        Self {
            config: crate::services::ServiceConfig::new(skills_dir, manifest_path),
        }
    }

    /// Install a skill from a source.
    ///
    /// Encapsulates business logic: parsing source, cloning,
    /// detecting repository type, validating SKILL.md, installing.
    /// `picker` replaces the interactive multi-select for multi-skill
    /// repositories; `None` falls back to the TUI.
    pub fn install(
        &self,
        source: &str,
        path_override: Option<String>,
        force: bool,
        sparse: bool,
        picker: Option<&mut SkillPicker>,
    ) -> Result<crate::core::install::InstallResult> {
        use crate::core::{git, install, repo_detect};

        let parsed = parse_source::Source::parse(source)?;

        // File URL path: download file, git not needed
        if parsed.is_file_url() {
            return self.install_file_url(&parsed, path_override, force);
        }

        let git_url = parsed.git_url()?;
        if !git::is_installed() {
            anyhow::bail!("Git is not installed. Please install git to use tome.");
        }
        let label = parsed.label().to_string();
        let skill_name = parsed.skill_name().map(String::from);

        // Clone repository
        let temp_dir = crate::fs::create_temp_dir()?;
        let do_sparse = sparse && path_override.is_none();

        git::clone_repo_or_sparse(
            &git_url,
            temp_dir.path(),
            do_sparse,
            git::GitOutput::Progress,
        )
        .with_context(|| format!("Failed to clone {}", git_url))?;

        let commit =
            git::get_commit_hash(temp_dir.path()).with_context(|| "Failed to get commit hash")?;

        let branch =
            git::get_branch(temp_dir.path()).unwrap_or_else(|_| crate::DEFAULT_BRANCH.to_string());

        let remote = git::get_remote_url(temp_dir.path()).unwrap_or_else(|_| git_url.clone());

        // Detect repository type and resolve skills
        let selected_skills = if let Some(custom_path) = &path_override {
            let repo_path = crate::fs::resolve_path_within(temp_dir.path(), custom_path)
                .with_context(|| format!("Invalid skill path '{custom_path}'"))?;
            let frontmatter = spec::validate_skill(&repo_path).context("Invalid SKILL.md")?;
            vec![(repo_path, frontmatter)]
        } else {
            let skills = repo_detect::resolve_skills(temp_dir.path())
                .context("Failed to detect repository structure")?;

            let is_single = matches!(skills.as_slice(), [_]);
            let selected = match skills.as_slice() {
                [skill] => {
                    if let Some(ref requested) = skill_name
                        && requested != &skill.name
                    {
                        anyhow::bail!("Requested skill '{}' but found '{}'", requested, skill.name);
                    }
                    vec![skill.clone()]
                }
                _ => match &skill_name {
                    Some(requested) => {
                        let skill =
                            skills
                                .iter()
                                .find(|s| s.name == *requested)
                                .ok_or_else(|| {
                                    anyhow::anyhow!(
                                        "Skill '{}' not found in repository. Available: {}",
                                        requested,
                                        skills
                                            .iter()
                                            .map(|s| s.name.as_str())
                                            .collect::<Vec<_>>()
                                            .join(", ")
                                    )
                                })?;
                        vec![skill.clone()]
                    }
                    None => match picker {
                        Some(pick) => pick(&skills)?,
                        None => select_skills(&skills, &label)?,
                    },
                },
            };

            let mut result = Vec::new();
            for skill in selected {
                let frontmatter = if is_single {
                    spec::validate_skill(&skill.path).context("Invalid SKILL.md")?
                } else {
                    spec::validate_skill(&skill.path)
                        .context(format!("Invalid SKILL.md for '{}'", skill.name))?
                };
                result.push((skill.path, frontmatter));
            }
            result
        };

        // Resolve to install format and install
        let resolved: Vec<install::ResolvedSkill> = selected_skills
            .iter()
            .map(|(repo_path, fm)| {
                install::ResolvedSkill::from_detected(
                    repo_path.clone(),
                    temp_dir.path(),
                    fm.clone(),
                    label.clone(),
                    commit.clone(),
                    remote.clone(),
                    branch.clone(),
                )
            })
            .collect();

        crate::fs::ensure_skills_dir(&self.config.skills_dir)?;
        install::install_skills(
            &resolved,
            force,
            &self.config.skills_dir,
            &self.config.manifest_path,
        )
    }

    /// Install skill from file:// URL.
    fn install_file_url(
        &self,
        parsed: &parse_source::Source,
        path_override: Option<String>,
        force: bool,
    ) -> Result<crate::core::install::InstallResult> {
        use crate::core::install;

        let url = match parsed {
            parse_source::Source::FileUrl { url } => url.clone(),
            _ => anyhow::bail!("Expected file URL"),
        };

        let downloaded = install::download_skill_file(&url)?;

        let repo_path = if let Some(ref custom_path) = path_override {
            crate::fs::resolve_path_within(&downloaded.path, custom_path)
                .with_context(|| format!("Invalid skill path '{custom_path}'"))?
        } else {
            downloaded.path
        };

        let frontmatter =
            spec::validate_skill(&repo_path).context("Downloaded file is not a valid SKILL.md")?;

        let path_value = path_override.unwrap_or_default();
        let resolved = vec![install::ResolvedSkill::from_file_url(
            repo_path,
            frontmatter,
            parsed.label(),
            path_value,
            url,
        )];

        crate::fs::ensure_skills_dir(&self.config.skills_dir)?;
        install::install_skills(
            &resolved,
            force,
            &self.config.skills_dir,
            &self.config.manifest_path,
        )
    }

    /// Resolve a removal target before prompting the user.
    pub(crate) fn resolve_removal_names(&self, name: &str) -> Result<Vec<String>> {
        let manifest = crate::core::manifest::Manifest::load(&self.config.manifest_path)?;
        let names = manifest.resolve_names(name);
        if names.is_empty() {
            anyhow::bail!(
                "Skill '{}' is not installed by tome. Use `rm -rf` to remove manually-placed skills.",
                name
            );
        }
        Ok(names)
    }

    /// Remove a skill and return all manifest names affected by the target.
    pub fn remove(&self, name: &str) -> Result<Vec<String>> {
        let names = self.resolve_removal_names(name)?;
        self.remove_names(&names)?;
        Ok(names)
    }

    /// Remove exact manifest keys, validating the entire selection before mutation.
    pub fn remove_selected(&self, names: &[String]) -> Result<Vec<String>> {
        let mut exact = names.to_vec();
        exact.sort();
        exact.dedup();
        self.remove_names(&exact)?;
        Ok(exact)
    }

    fn remove_names(&self, names: &[String]) -> Result<()> {
        let manifest_path = &self.config.manifest_path;
        let mut manifest = crate::core::manifest::Manifest::load(manifest_path)?;
        for name in names {
            if !manifest.is_tracked(name) {
                anyhow::bail!("Skill '{}' is not installed by tome", name);
            }
        }
        if names.is_empty() {
            return Ok(());
        }
        let mut transaction = crate::fs::SkillTransaction::new(&self.config.skills_dir)?;
        for name in names {
            let skill_dir = self.config.skills_dir.join(name);
            transaction.remove(&skill_dir)?;
            manifest.remove_skill(name);
        }
        transaction.commit_with(|| manifest.save(manifest_path))
    }
}

fn select_skills(
    skills: &[repo_detect::SkillInfo],
    source: &str,
) -> Result<Vec<repo_detect::SkillInfo>> {
    if skills.len() == 1 {
        return Ok(vec![skills[0].clone()]);
    }

    let mut items = build_select_items(skills);

    for item in &mut items {
        item.source = source.into();
    }
    if crate::tui::run_list(
        "Select skills to install",
        &mut items,
        crate::tui::ListMode::Select,
    )? != crate::tui::ListOutcome::Accepted
    {
        anyhow::bail!("Selection cancelled");
    }

    let selected: Vec<repo_detect::SkillInfo> = skills
        .iter()
        .zip(items)
        .filter(|(_, item)| item.selected)
        .map(|(skill, _)| skill.clone())
        .collect();

    if selected.is_empty() {
        anyhow::bail!("No skills selected");
    }

    Ok(selected)
}

const LARGE_LIST_THRESHOLD: usize = 20;

fn default_selected(len: usize) -> bool {
    len <= LARGE_LIST_THRESHOLD
}

fn build_select_items(skills: &[repo_detect::SkillInfo]) -> Vec<crate::tui::ListItem> {
    let preselected = default_selected(skills.len());
    skills
        .iter()
        .map(|skill| crate::tui::ListItem {
            name: skill.name.clone(),
            description: skill.description.clone().unwrap_or_default(),
            selected: preselected,
            ..crate::tui::ListItem::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::{Manifest, SkillEntry};
    use crate::testutil;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn install_file_url_through_service_updates_manifest() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("SKILL.md");
        fs::write(
            &source,
            "---\nname: service-skill\ndescription: installed through service\n---\n",
        )
        .unwrap();
        let skills_dir = temp.path().join("skills");
        let manifest_path = skills_dir.join(".skills.json");
        let service = SkillService::with_manifest_path(skills_dir.clone(), manifest_path.clone());

        let source_url = format!("file://{}", source.display());
        let result = service
            .install(&source_url, None, false, true, None)
            .unwrap();

        assert_eq!(result.installed, vec!["service-skill"]);
        assert!(skills_dir.join("service-skill/SKILL.md").exists());
        assert!(
            Manifest::load(&manifest_path)
                .unwrap()
                .is_tracked("service-skill")
        );
    }

    #[test]
    fn remove_through_service_resolves_source_and_returns_names() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("first")).unwrap();
        fs::create_dir_all(skills_dir.join("second")).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        let mut manifest = Manifest::new();
        let frontmatter = crate::testutil::make_fm("skill");
        manifest.add_skill(
            "first",
            SkillEntry::new("owner/repo", "", &frontmatter, "abc", "remote"),
        );
        manifest.add_skill(
            "second",
            SkillEntry::new("owner/repo", "", &frontmatter, "def", "remote"),
        );
        manifest.save(&manifest_path).unwrap();
        let service = SkillService::with_manifest_path(skills_dir.clone(), manifest_path.clone());

        let mut removed = service.remove("owner/repo").unwrap();
        removed.sort();

        assert_eq!(removed, vec!["first", "second"]);
        assert!(!skills_dir.join("first").exists());
        assert!(!skills_dir.join("second").exists());
        assert!(Manifest::load(&manifest_path).unwrap().is_empty());
    }

    #[test]
    fn install_rejects_invalid_source_through_service() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let manifest_path = skills_dir.join(".skills.json");
        let service = SkillService::with_manifest_path(skills_dir, manifest_path);

        let error = match service.install("not-a-source", None, false, true, None) {
            Ok(_) => panic!("invalid source should fail"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("owner/repo"));
    }

    #[test]
    fn install_with_picker_selects_skills_without_tui() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join("skills/first")).unwrap();
        fs::create_dir_all(repo.join("skills/second")).unwrap();
        fs::write(
            repo.join("skills/first/SKILL.md"),
            "---\nname: first\ndescription: one\nversion: 1.0.0\n---\n",
        )
        .unwrap();
        fs::write(
            repo.join("skills/second/SKILL.md"),
            "---\nname: second\ndescription: two\nversion: 1.0.0\n---\n",
        )
        .unwrap();
        testutil::init_git_repo(&repo);

        let skills_dir = temp.path().join("skills");
        let manifest_path = skills_dir.join(".skills.json");
        let service = SkillService::with_manifest_path(skills_dir.clone(), manifest_path.clone());

        let mut picker = |skills: &[repo_detect::SkillInfo]| {
            Ok(skills
                .iter()
                .filter(|s| s.name == "first")
                .cloned()
                .collect())
        };
        let source_url = format!("file://{}", repo.display());
        let result = service
            .install(&source_url, None, false, true, Some(&mut picker))
            .unwrap();

        assert_eq!(result.installed, vec!["first v1.0.0"]);
        assert!(!skills_dir.join("second").exists());
    }

    fn fake_skill(name: &str) -> repo_detect::SkillInfo {
        repo_detect::SkillInfo {
            name: name.to_string(),
            path: std::path::PathBuf::from(name),
            description: Some(format!("{name} desc")),
        }
    }

    #[test]
    fn test_default_selected_threshold_boundary() {
        assert!(super::default_selected(1));
        assert!(super::default_selected(20));
        assert!(!super::default_selected(21));
        assert!(!super::default_selected(429));
    }

    #[test]
    fn test_build_select_items_small_preselects_all() {
        let skills = vec![fake_skill("a"), fake_skill("b"), fake_skill("c")];
        let items = super::build_select_items(&skills);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].name, "a");
        assert_eq!(items[0].description, "a desc");
        assert!(items.iter().all(|i| i.selected));
    }

    #[test]
    fn test_build_select_items_large_preselects_none() {
        let skills: Vec<_> = (0..25).map(|i| fake_skill(&format!("s{i}"))).collect();
        let items = super::build_select_items(&skills);
        assert_eq!(items.len(), 25);
        assert!(items.iter().all(|i| !i.selected));
    }

    #[test]
    fn test_build_select_items_missing_description_defaults_empty() {
        let skill = repo_detect::SkillInfo {
            name: "x".to_string(),
            path: std::path::PathBuf::from("x"),
            description: None,
        };
        let items = super::build_select_items(std::slice::from_ref(&skill));
        assert_eq!(items[0].description, "");
    }
    #[test]
    fn scalable_removal_validates_entire_exact_batch_before_deletion() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(skills_dir.join("keep")).unwrap();
        let path = skills_dir.join(".skills.json");
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "keep",
            crate::core::manifest::SkillEntry::new(
                "owner/repo",
                "",
                &crate::testutil::make_fm("keep"),
                "abc",
                "file:///repo",
            ),
        );
        manifest.save(&path).unwrap();
        let service = SkillService::with_manifest_path(skills_dir.clone(), path.clone());
        assert!(
            service
                .remove_names(&["keep".into(), "unknown".into()])
                .is_err()
        );
        assert!(skills_dir.join("keep").exists());
        assert!(Manifest::load(&path).unwrap().is_tracked("keep"));
    }
    #[test]
    fn remove_rolls_back_when_manifest_save_fails() {
        let temp = TempDir::new().unwrap();
        let skills = temp.path().join("skills");
        fs::create_dir_all(skills.join("first")).unwrap();
        fs::write(skills.join("first/SKILL.md"), "original").unwrap();
        // A supported custom manifest location inside a removed skill causes save to fail.
        let path = skills.join("first/.skills.json");
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "first",
            SkillEntry::new(
                "owner/repo",
                "",
                &testutil::make_fm("first"),
                "abc",
                "remote",
            ),
        );
        manifest.save(&path).unwrap();
        let before = fs::read(&path).unwrap();
        let service = SkillService::with_manifest_path(skills.clone(), path.clone());
        assert!(service.remove("first").is_err());
        assert_eq!(
            fs::read_to_string(skills.join("first/SKILL.md")).unwrap(),
            "original"
        );
        assert_eq!(fs::read(path).unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn remove_unlinks_dangling_skill_symlink() {
        let temp = TempDir::new().unwrap();
        let skills = temp.path().join("skills");
        fs::create_dir_all(&skills).unwrap();
        std::os::unix::fs::symlink(temp.path().join("missing"), skills.join("linked")).unwrap();
        let path = skills.join(".skills.json");
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "linked",
            SkillEntry::new("source", "", &testutil::make_fm("linked"), "abc", "remote"),
        );
        manifest.save(&path).unwrap();
        let service = SkillService::with_manifest_path(skills.clone(), path.clone());
        service.remove("linked").unwrap();
        assert!(fs::symlink_metadata(skills.join("linked")).is_err());
        assert!(!Manifest::load(&path).unwrap().is_tracked("linked"));
    }
}
