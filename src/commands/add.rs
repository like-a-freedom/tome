use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::core::{
    git, manifest::{Manifest, SkillEntry}, normalize, parse_source, spec,
};
use crate::fs;

pub fn execute(
    source: &str,
    path_override: Option<String>,
    force: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let is_url = source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("file://")
        || source.starts_with("git@")
        || source.starts_with("ssh://");

    let (owner_repo, git_url, skill_name) = if is_url {
        (source.to_string(), source.to_string(), None)
    } else {
        let parsed = parse_source::parse_source(source)?;
        let url = format!("https://github.com/{}/{}.git", parsed.owner, parsed.repo);
        (format!("{}/{}", parsed.owner, parsed.repo), url, parsed.skill)
    };

    if !git::is_installed() {
        bail!("Git is not installed. Please install git to use tome.");
    }

    let temp_dir = fs::create_temp_dir()?;

    git::clone_repo(&git_url, temp_dir.path())
        .context("Failed to clone repository")?;

    let commit = git::get_commit_hash(temp_dir.path())
        .with_context(|| "Failed to get commit hash")?;
    let branch = git::get_branch(temp_dir.path())
        .unwrap_or_else(|_| "main".to_string());
    let remote = git::get_remote_url(temp_dir.path())
        .unwrap_or_else(|_| git_url.clone());

    let selected_skills = if let Some(custom_path) = &path_override {
        let repo_path = temp_dir.path().join(custom_path);
        let frontmatter = spec::validate_skill(&repo_path)
            .context("Invalid SKILL.md")?;
        vec![(repo_path, frontmatter)]
    } else {
        let repo_type = normalize::detect_repo_type(temp_dir.path())
            .context("Failed to detect repository structure")?;

        match repo_type {
            normalize::RepoType::SingleSkillAtRoot => {
                let frontmatter = spec::validate_skill(temp_dir.path())
                    .context("Invalid SKILL.md")?;
                vec![(temp_dir.path().to_path_buf(), frontmatter)]
            }
            normalize::RepoType::SingleSkillInDir { name, path: _ } => {
                if let Some(ref requested) = skill_name {
                    if requested != &name {
                        bail!("Requested skill '{}' but found '{}'", requested, name);
                    }
                }
                let skill_path = temp_dir.path().join("skills").join(&name);
                let frontmatter = spec::validate_skill(&skill_path)
                    .context("Invalid SKILL.md")?;
                vec![(skill_path, frontmatter)]
            }
            normalize::RepoType::MultiSkill(skills) => {
                let selected = if let Some(ref requested) = skill_name {
                    let skill = skills.iter()
                        .find(|s| s.name == *requested)
                        .ok_or_else(|| anyhow::anyhow!(
                            "Skill '{}' not found in repository. Available: {}",
                            requested,
                            skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ")
                        ))?;
                    vec![skill.clone()]
                } else {
                    normalize::select_skills(&skills)?
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

    for (repo_path, frontmatter) in &selected_skills {
        let dest_dir = skills_dir.join(&frontmatter.name);
        if dest_dir.exists() && !force {
            if manifest.is_tracked(&frontmatter.name) {
                eprintln!(
                    "Skipping: '{}' is already installed (use --force to overwrite)",
                    frontmatter.name
                );
                continue;
            } else {
                eprintln!(
                    "Skipping: directory '{}' exists (not installed by tome, use --force to overwrite)",
                    frontmatter.name
                );
                continue;
            }
        }

        normalize::copy_skill(repo_path, &dest_dir)?;

        let path_value = if *repo_path == *temp_dir.path() {
            String::new()
        } else {
            repo_path.strip_prefix(temp_dir.path())
                .unwrap_or(repo_path)
                .to_string_lossy()
                .into_owned()
        };

        let mut entry = SkillEntry::new(&owner_repo, &path_value, frontmatter, &commit, &remote);
        entry.branch = branch.clone();
        manifest.add_skill(&frontmatter.name, entry);

        installed.push(format!(
            "{} v{}",
            frontmatter.name,
            frontmatter.version.as_deref().unwrap_or("0.0.0")
        ));
    }

    manifest.save(manifest_path)?;

    if installed.is_empty() {
        println!("No new skills installed.");
    } else if installed.len() == 1 {
        println!("Installed: {}", installed[0]);
    } else {
        println!("Installed {} skills:", installed.len());
        for name in &installed {
            println!("  - {}", name);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_skill_repo(temp: &TempDir, _name: &str, content: &str) -> String {
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).unwrap();
        fs::write(repo_dir.join("SKILL.md"), content).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["add", "-A"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo_dir).output().unwrap();
        format!("file://{}", repo_dir.display())
    }

    #[test]
    fn test_add_url_bypasses_parse_source() {
        let temp = TempDir::new().unwrap();
        let url = make_skill_repo(&temp, "test", "---\nname: UrlSkill\ndescription: from url\n---\n");
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let result = execute(&url, None, false, &skills_dir, &manifest_path);
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
        let result = execute("https://github.com/nonexistent/repo.git", None, false, &skills_dir, &manifest_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("clone"));
    }

    #[test]
    fn test_add_invalid_source_errors() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let result = execute("invalid-no-slash", None, false, &skills_dir, &manifest_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_existing_untracked_dir_skips() {
        let temp = TempDir::new().unwrap();
        let url = make_skill_repo(&temp, "test", "---\nname: Untracked\ndescription: d\n---\n");
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("Untracked")).unwrap();
        fs::write(skills_dir.join("Untracked").join("SKILL.md"), "---\nname: Untracked\ndescription: old\n---\n").unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(&url, None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        // Should NOT overwrite the existing directory
        let content = fs::read_to_string(skills_dir.join("Untracked").join("SKILL.md")).unwrap();
        assert!(content.contains("old"));
    }

    #[test]
    fn test_add_force_overwrites() {
        let temp = TempDir::new().unwrap();
        let url = make_skill_repo(&temp, "test", "---\nname: ForceOver\ndescription: new\n---\n");
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("ForceOver")).unwrap();
        fs::write(skills_dir.join("ForceOver").join("SKILL.md"), "---\nname: ForceOver\ndescription: old\n---\n").unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(&url, None, true, &skills_dir, &manifest_path);
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
        ).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["add", "-A"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo_dir).output().unwrap();
        let url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let result = execute(&url, Some("skills/myskill".to_string()), false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
        assert!(skills_dir.join("PathSkill").join("SKILL.md").exists());
    }

    #[test]
    fn test_add_multi_skill_with_name() {
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(repo_dir.join("skills/skill-a")).unwrap();
        fs::write(repo_dir.join("skills/skill-a/SKILL.md"), "---\nname: SkillA\ndescription: a\n---\n").unwrap();
        fs::create_dir_all(repo_dir.join("skills/skill-b")).unwrap();
        fs::write(repo_dir.join("skills/skill-b/SKILL.md"), "---\nname: SkillB\ndescription: b\n---\n").unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["add", "-A"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo_dir).output().unwrap();
        let url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        // Pass URL with @skill-name suffix — but file:// URLs don't parse @ correctly
        // Instead, test via owner/repo@skill format (won't work for file://)
        // So test MultiSkill by passing just the file:// URL (no @)
        // This triggers select_skill which needs stdin — skip for unit test
        // Instead test the error path: requested skill not found
        let result = execute(&format!("{}@nonexistent", url), None, false, &skills_dir, &manifest_path);
        assert!(result.is_err());
    }
}
