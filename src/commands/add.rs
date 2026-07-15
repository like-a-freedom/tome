use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::core::{
    download, git,
    manifest::{Manifest, SkillEntry},
    parse_source, repo_detect, spec,
};
use crate::fs;
use crate::style::{self, CYAN, DIM, GREEN, YELLOW};

pub fn execute(
    source: &str,
    path_override: Option<String>,
    force: bool,
    sparse: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let parsed_source = parse_source::Source::parse(source)?;

    // File URL path: download single file, skip git entirely
    if parsed_source.is_file_url() {
        return execute_file_url(
            &parsed_source,
            path_override,
            force,
            skills_dir,
            manifest_path,
        );
    }

    let git_url = parsed_source.git_url()?;
    let label = parsed_source.label();
    let skill_name = parsed_source.skill_name().map(String::from);

    if !git::is_installed() {
        bail!("Git is not installed. Please install git to use tome.");
    }

    // Show what we're cloning
    eprintln!(
        "\n{} {}Cloning{} {}",
        style::styled(CYAN, "\u{25b8}"),
        style::BOLD,
        style::RESET,
        label,
    );

    let temp_dir = fs::create_temp_dir()?;

    // When --path is specified, use a full clone (the target
    // may be outside skills/ or root SKILL.md)
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

    let selected_skills = if let Some(custom_path) = &path_override {
        let repo_path = temp_dir.path().join(custom_path);
        let frontmatter = spec::validate_skill(&repo_path).context("Invalid SKILL.md")?;
        vec![(repo_path, frontmatter)]
    } else {
        let repo_type = repo_detect::detect_repo_type(temp_dir.path())
            .context("Failed to detect repository structure")?;

        match repo_type {
            repo_detect::RepoType::SingleSkillAtRoot => {
                let frontmatter =
                    spec::validate_skill(temp_dir.path()).context("Invalid SKILL.md")?;
                vec![(temp_dir.path().to_path_buf(), frontmatter)]
            }
            repo_detect::RepoType::SingleSkillInDir { name, path } => {
                if let Some(ref requested) = skill_name
                    && requested != &name
                {
                    bail!("Requested skill '{}' but found '{}'", requested, name);
                }
                let frontmatter = spec::validate_skill(&path).context("Invalid SKILL.md")?;
                vec![(path, frontmatter)]
            }
            repo_detect::RepoType::MultiSkill(skills) => {
                let selected = if let Some(ref requested) = skill_name {
                    let skill = skills
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
                } else {
                    select_skills(&skills)?
                };

                let mut result = Vec::new();
                for skill in selected {
                    let frontmatter = spec::validate_skill(&skill.path)
                        .context(format!("Invalid SKILL.md for '{}'", skill.name))?;
                    result.push((skill.path, frontmatter));
                }
                result
            }
        }
    };

    fs::ensure_skills_dir(skills_dir)?;
    let mut manifest = Manifest::load(manifest_path)?;
    let mut installed = Vec::new();
    let mut skipped_tracked = Vec::new();
    let mut skipped_existing = Vec::new();

    for (repo_path, frontmatter) in &selected_skills {
        let dest_dir = skills_dir.join(&frontmatter.name);
        if dest_dir.exists() && !force {
            if manifest.is_tracked(&frontmatter.name) {
                skipped_tracked.push(frontmatter.name.clone());
                continue;
            } else {
                skipped_existing.push(frontmatter.name.clone());
                continue;
            }
        }

        fs::copy_skill(repo_path, &dest_dir)?;

        let path_value = if *repo_path == *temp_dir.path() {
            String::new()
        } else {
            repo_path
                .strip_prefix(temp_dir.path())
                .unwrap_or(repo_path)
                .to_string_lossy()
                .into_owned()
        };

        let mut entry = SkillEntry::new(&label, &path_value, frontmatter, &commit, &remote);
        entry.set_branch(&branch);
        manifest.add_skill(&frontmatter.name, entry);

        let version_tag = frontmatter
            .version
            .as_deref()
            .filter(|v| !v.is_empty())
            .map(|v| format!(" v{}", v))
            .unwrap_or_default();
        installed.push(format!("{}{}", frontmatter.name, version_tag));
    }

    manifest.save(manifest_path)?;

    // --- Output ---
    if !installed.is_empty() {
        eprintln!();
        if installed.len() == 1 {
            eprintln!(
                "  {} Installed {}{}{}",
                style::styled(GREEN, "\u{2713}"),
                style::BOLD,
                installed[0],
                style::RESET,
            );
        } else {
            eprintln!(
                "  {} Installed {} skills:\n",
                style::styled(GREEN, "\u{2713}"),
                installed.len(),
            );

            for name in &installed {
                eprintln!("    {}", name);
            }
        }
    }

    for name in &skipped_tracked {
        eprintln!(
            "  {} {} is already installed (use --force to overwrite)",
            style::styled(DIM, "\u{2713}"),
            name,
        );
    }

    for name in &skipped_existing {
        eprintln!(
            "  {} {} directory exists (not tracked, use --force)",
            style::styled(YELLOW, "\u{26a0}"),
            name,
        );
    }

    if installed.is_empty() && skipped_tracked.is_empty() && skipped_existing.is_empty() {
        eprintln!("  No new skills installed.");
    }

    Ok(())
}

fn execute_file_url(
    source: &parse_source::Source,
    path_override: Option<String>,
    force: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let url = match source {
        parse_source::Source::FileUrl { url } => url.clone(),
        _ => anyhow::bail!("Expected file URL"),
    };

    let label = source.label();

    eprintln!(
        "\n{} {}Downloading{} {}",
        style::styled(CYAN, "\u{25b8}"),
        style::BOLD,
        style::RESET,
        label,
    );

    let downloaded = download::download_skill_file(&url)?;

    let repo_path = if let Some(ref custom_path) = path_override {
        downloaded.path.join(custom_path)
    } else {
        downloaded.path
    };

    let frontmatter =
        spec::validate_skill(&repo_path).context("Downloaded file is not a valid SKILL.md")?;

    fs::ensure_skills_dir(skills_dir)?;
    let mut manifest = Manifest::load(manifest_path)?;

    let dest_dir = skills_dir.join(&frontmatter.name);
    if dest_dir.exists() && !force {
        if manifest.is_tracked(&frontmatter.name) {
            eprintln!(
                "  {} {} is already installed (use --force to overwrite)",
                style::styled(DIM, "\u{2713}"),
                frontmatter.name,
            );
            return Ok(());
        } else {
            eprintln!(
                "  {} {} directory exists (not tracked, use --force)",
                style::styled(YELLOW, "\u{26a0}"),
                frontmatter.name,
            );
            return Ok(());
        }
    }

    fs::copy_skill(&repo_path, &dest_dir)?;

    let path_value = path_override.unwrap_or_default();

    let mut entry = SkillEntry::new(&label, &path_value, &frontmatter, "file:", &url);
    entry.set_branch("");
    manifest.add_skill(&frontmatter.name, entry);
    manifest.save(manifest_path)?;

    let version_tag = frontmatter
        .version
        .as_deref()
        .filter(|v| !v.is_empty())
        .map(|v| format!(" v{}", v))
        .unwrap_or_default();

    let skill_display = format!("{}{}", frontmatter.name, version_tag);

    eprintln!();
    eprintln!(
        "  {} Installed {}{}{}",
        style::styled(GREEN, "\u{2713}"),
        style::BOLD,
        skill_display,
        style::RESET,
    );

    Ok(())
}

fn select_skills(skills: &[repo_detect::SkillInfo]) -> Result<Vec<repo_detect::SkillInfo>> {
    if skills.len() == 1 {
        return Ok(vec![skills[0].clone()]);
    }

    let mut items: Vec<crate::tui::SelectItem> = skills
        .iter()
        .map(|s| crate::tui::SelectItem {
            name: s.name.clone(),
            description: s.description.clone().unwrap_or_default(),
            selected: true,
        })
        .collect();

    crate::tui::multi_select(&mut items)?;

    let selected: Vec<repo_detect::SkillInfo> = skills
        .iter()
        .zip(items.iter())
        .filter(|(_, item)| item.selected)
        .map(|(skill, _)| skill.clone())
        .collect();

    if selected.is_empty() {
        bail!("No skills selected");
    }

    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_add_url_bypasses_parse_source() {
        let temp = TempDir::new().unwrap();
        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: UrlSkill\ndescription: from url\n---\n",
        );
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let result = execute(&url, None, false, true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
        assert!(skills_dir.join("UrlSkill").join("SKILL.md").exists());
    }

    #[test]
    fn test_add_https_url_prefix() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        // This will fail to clone (invalid URL) but should not fail on parse_source
        let result = execute(
            "https://github.com/nonexistent/repo.git",
            None,
            false,
            true,
            &skills_dir,
            &manifest_path,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("clone"));
    }

    #[test]
    fn test_add_invalid_source_errors() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let result = execute(
            "invalid-no-slash",
            None,
            false,
            true,
            &skills_dir,
            &manifest_path,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_add_existing_untracked_dir_skips() {
        let temp = TempDir::new().unwrap();
        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: Untracked\ndescription: d\n---\n",
        );
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("Untracked")).unwrap();
        fs::write(
            skills_dir.join("Untracked").join("SKILL.md"),
            "---\nname: Untracked\ndescription: old\n---\n",
        )
        .unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(&url, None, false, true, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        // Should NOT overwrite the existing directory
        let content = fs::read_to_string(skills_dir.join("Untracked").join("SKILL.md")).unwrap();
        assert!(content.contains("old"));
    }

    #[test]
    fn test_add_force_overwrites() {
        let temp = TempDir::new().unwrap();
        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: ForceOver\ndescription: new\n---\n",
        );
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("ForceOver")).unwrap();
        fs::write(
            skills_dir.join("ForceOver").join("SKILL.md"),
            "---\nname: ForceOver\ndescription: old\n---\n",
        )
        .unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(&url, None, true, true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_add_with_path_override() {
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(repo_dir.join("skills/myskill")).unwrap();
        fs::write(
            repo_dir.join("skills/myskill/SKILL.md"),
            "---\nname: PathSkill\ndescription: custom path\n---\n",
        )
        .unwrap();
        testutil::init_git_repo(&repo_dir);
        let url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let result = execute(
            &url,
            Some("skills/myskill".to_string()),
            false,
            true,
            &skills_dir,
            &manifest_path,
        );
        assert!(result.is_ok());
        assert!(skills_dir.join("PathSkill").join("SKILL.md").exists());
    }

    #[test]
    fn test_add_multi_skill_with_name() {
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(repo_dir.join("skills/skill-a")).unwrap();
        fs::write(
            repo_dir.join("skills/skill-a/SKILL.md"),
            "---\nname: SkillA\ndescription: a\n---\n",
        )
        .unwrap();
        fs::create_dir_all(repo_dir.join("skills/skill-b")).unwrap();
        fs::write(
            repo_dir.join("skills/skill-b/SKILL.md"),
            "---\nname: SkillB\ndescription: b\n---\n",
        )
        .unwrap();
        testutil::init_git_repo(&repo_dir);
        let url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        // Test the error path: requested skill not found
        let result = execute(
            &format!("{}@nonexistent", url),
            None,
            false,
            true,
            &skills_dir,
            &manifest_path,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_add_uses_sparse_clone_for_large_repo() {
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(repo_dir.join("skills/SparseSkill")).unwrap();
        fs::write(
            repo_dir.join("skills/SparseSkill/SKILL.md"),
            "---\nname: SparseSkill\ndescription: sparse test\n---\n",
        )
        .unwrap();
        fs::write(repo_dir.join("large.bin"), vec![0u8; 1024 * 1024]).unwrap();
        testutil::init_git_repo(&repo_dir);
        let url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let result = execute(&url, None, false, true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
        assert!(skills_dir.join("SparseSkill").join("SKILL.md").exists());
    }

    #[test]
    fn test_add_no_sparse_uses_full_clone() {
        let temp = TempDir::new().unwrap();
        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: FullClone\ndescription: test\n---\n",
        );
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let result = execute(&url, None, false, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
        assert!(skills_dir.join("FullClone").join("SKILL.md").exists());
    }

    #[test]
    fn test_add_file_url_invalid_skill_md() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        // Create a file that is NOT a valid SKILL.md (no frontmatter)
        let bad_file = temp.path().join("not-a-skill.md");
        fs::write(&bad_file, "Just some plain text, no frontmatter.\n").unwrap();
        let file_url = format!("file://{}", bad_file.display());

        let result = execute(&file_url, None, false, true, &skills_dir, &manifest_path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid SKILL.md") || err.contains("not a valid"),
            "Expected SKILL.md validation error, got: {}",
            err
        );
    }

    #[test]
    fn test_select_skill_single_returns_first() {
        let skills = vec![repo_detect::SkillInfo {
            name: "only".to_string(),
            path: std::path::PathBuf::from("/tmp"),
            description: Some("desc".to_string()),
        }];
        let result = select_skills(&skills).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "only");
    }
}
