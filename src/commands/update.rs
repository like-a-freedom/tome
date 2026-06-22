use anyhow::{bail, Result};
use std::path::Path;

use crate::core::{git, manifest::Manifest, normalize, spec};
use crate::fs;

pub fn execute(
    name: Option<&str>,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let mut manifest = Manifest::load(manifest_path)?;

    let stale = manifest.clean_stale(skills_dir);
    if !stale.is_empty() {
        eprintln!(
            "Skipping {} stale entries: {}",
            stale.len(),
            stale.join(", ")
        );
    }

    let skills_to_update: Vec<String> = if let Some(specific_name) = name {
        if !manifest.is_tracked(specific_name) {
            bail!("Skill '{}' not found in manifest.", specific_name);
        }
        vec![specific_name.to_string()]
    } else {
        if manifest.skills.is_empty() {
            println!("No skills to update.");
            return Ok(());
        }
        manifest.skills.keys().cloned().collect()
    };

    for skill_name in &skills_to_update {
        let entry = manifest.skills.get(skill_name).unwrap();
        let remote = entry.remote.clone();

        let temp_dir = fs::create_temp_dir()?;

        println!("Updating {} from {}...", skill_name, remote);
        if let Err(e) = git::clone_repo(&remote, temp_dir.path()) {
            eprintln!("Warning: Failed to update {}: {}", skill_name, e);
            continue;
        }

        let skill_repo_path = if entry.path.is_empty() {
            match normalize::detect_repo_type(temp_dir.path()) {
                Ok(normalize::RepoType::SingleSkillAtRoot) => temp_dir.path().to_path_buf(),
                Ok(normalize::RepoType::SingleSkillInDir { name: _, path: p }) => p,
                _ => temp_dir.path().to_path_buf(),
            }
        } else {
            temp_dir.path().join(&entry.path)
        };

        let frontmatter = match spec::validate_skill(&skill_repo_path) {
            Ok(fm) => fm,
            Err(e) => {
                eprintln!("Warning: Skipping {} — invalid updated SKILL.md: {}", skill_name, e);
                continue;
            }
        };

        let commit = git::get_commit_hash(temp_dir.path())
            .unwrap_or_else(|_| "unknown".to_string());
        let branch = git::get_branch(temp_dir.path())
            .unwrap_or_else(|_| "main".to_string());

        let dest_dir = skills_dir.join(skill_name);
        if let Err(e) = normalize::copy_skill(&skill_repo_path, &dest_dir) {
            eprintln!("Warning: Failed to copy updated files for {}: {}", skill_name, e);
            continue;
        }

        if let Some(entry) = manifest.skills.get_mut(skill_name) {
            entry.update(&commit, &frontmatter.version.unwrap_or("0.0.0".into()));
            entry.branch = branch;
            entry.description = frontmatter.description;
            println!("  Updated: {} -> {} ({})", skill_name, entry.version, commit);
        }
    }

    manifest.save(manifest_path)?;

    println!("Update complete. {} skill(s) updated.", skills_to_update.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::{Manifest, SkillEntry};
    use crate::core::spec::SkillFrontmatter;
    use std::fs;
    use tempfile::TempDir;

    fn make_fm(name: &str) -> SkillFrontmatter {
        SkillFrontmatter {
            name: name.to_string(),
            description: format!("{} desc", name),
            version: Some("1.0.0".into()),
            authors: None,
            tags: None,
            platforms: None,
            license: None,
            repository: None,
            docs: None,
        }
    }

    fn make_entry(remote: &str) -> SkillEntry {
        SkillEntry::new("test/repo", "skills/x", &make_fm("x"), "abc", remote)
    }

    #[test]
    fn test_update_empty_manifest_returns_ok() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(None, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_specific_not_found_errors() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(Some("nonexistent"), &skills_dir, &manifest_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_update_stale_entries_cleaned() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        // Manifest references a skill dir that doesn't exist
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        manifest.add_skill("ghost", make_entry("http://example.com/fake.git"));
        manifest.save(&manifest_path).unwrap();

        // execute will try to clone from the remote, which will fail, but stale entries are cleaned
        let result = execute(None, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_with_local_repo_success() {
        // Create a local git repo with a skill at root (SingleSkillAtRoot pattern)
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).unwrap();
        fs::write(
            repo_dir.join("SKILL.md"),
            "---\nname: TestSkill\ndescription: test\nversion: 2.0.0\n---\n",
        ).unwrap();
        std::process::Command::new("git").args(["init"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["config", "user.name", "T"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["add", "-A"]).current_dir(&repo_dir).output().unwrap();
        std::process::Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo_dir).output().unwrap();

        let remote_url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        // entry.path is empty (skill at root) so detect_repo_type will find it
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        let mut entry = SkillEntry::new("test/repo", "", &make_fm("TestSkill"), "oldhash", &remote_url);
        entry.branch = "main".to_string();
        manifest.add_skill("TestSkill", entry);
        manifest.save(&manifest_path).unwrap();

        // Create skill dir so it's not treated as stale
        fs::create_dir_all(skills_dir.join("TestSkill")).unwrap();
        fs::write(
            skills_dir.join("TestSkill").join("SKILL.md"),
            "---\nname: TestSkill\ndescription: old\nversion: 1.0.0\n---\n",
        ).unwrap();

        let result = execute(None, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        // Verify manifest was updated
        let loaded = Manifest::load(&manifest_path).unwrap();
        let entry = loaded.skills.get("TestSkill").unwrap();
        assert_eq!(entry.version, "2.0.0");
    }
}
