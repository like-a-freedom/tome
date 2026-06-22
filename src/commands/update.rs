use anyhow::{bail, Result};
use std::path::Path;

use crate::core::{git, manifest::Manifest, normalize, spec};
use crate::fs;

fn format_commit(commit: &str) -> String {
    if commit.len() >= 7 {
        commit[..7].to_string()
    } else {
        commit.to_string()
    }
}

fn format_date(rfc3339: &str) -> String {
    // Parse RFC 3339 and return YYYY-MM-DD
    if let Some(date_part) = rfc3339.split('T').next() {
        date_part.to_string()
    } else {
        rfc3339.to_string()
    }
}

fn print_concise(skills: &[(String, String, String, String)]) {
    // skills: (name, old_commit, new_commit, date)
    if skills.is_empty() {
        return;
    }

    // Find column widths for alignment
    let name_width = skills.iter().map(|(n, _, _, _)| n.len()).max().unwrap_or(10);
    let commit_width = 7; // hashes are 7 chars

    for (name, old_commit, new_commit, _date) in skills {
        println!(
            "  {:<width$}  {:>commit_width$} -> {}",
            name,
            format_commit(old_commit),
            format_commit(new_commit),
            width = name_width,
            commit_width = commit_width,
        );
    }
}

pub fn execute(
    name: Option<&str>,
    verbose: bool,
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

    let mut updated_skills: Vec<(String, String, String, String)> = Vec::new();
    // date is determined by the first successful update (all happen at roughly the same time)
    let mut update_date: Option<String> = None;

    for skill_name in &skills_to_update {
        let entry = manifest.skills.get(skill_name).unwrap();
        let remote = entry.remote.clone();
        let old_commit = entry.commit.clone();

        let temp_dir = fs::create_temp_dir()?;

        if verbose {
            println!("Updating {} from {}...", skill_name, remote);
        }

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
            entry.update(&commit, &frontmatter.version.unwrap_or_default());
            entry.branch = branch;
            entry.description = frontmatter.description;

            let date_str = format_date(&entry.updated);
            if update_date.is_none() {
                update_date = Some(date_str.clone());
            }

            if verbose {
                let ver = &entry.version;
                if ver.is_empty() {
                    println!("  Updated: {} ({} -> {})", skill_name, format_commit(&old_commit), format_commit(&commit));
                } else {
                    println!("  Updated: {} {} ({} -> {})", skill_name, ver, format_commit(&old_commit), format_commit(&commit));
                }
            }

            updated_skills.push((
                skill_name.clone(),
                old_commit,
                commit,
                date_str,
            ));
        }
    }

    manifest.save(manifest_path)?;

    if !verbose && !updated_skills.is_empty() {
        print_concise(&updated_skills);
    }

    let date_suffix = update_date
        .map(|d| format!(" at {}", d))
        .unwrap_or_default();
    println!("Update complete. {} skill(s) updated{}.",
        updated_skills.len(),
        date_suffix,
    );

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

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_specific_not_found_errors() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(Some("nonexistent"), false, &skills_dir, &manifest_path);
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
        let result = execute(None, false, &skills_dir, &manifest_path);
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

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        // Verify manifest was updated
        let loaded = Manifest::load(&manifest_path).unwrap();
        let entry = loaded.skills.get("TestSkill").unwrap();
        assert_eq!(entry.version, "2.0.0");
    }

    #[test]
    fn test_update_verbose_flag_accepted() {
        // Just check that verbose=true doesn't crash on empty manifest
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(None, true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_format_commit() {
        assert_eq!(format_commit("abcdef12345"), "abcdef1");
        assert_eq!(format_commit("abc"), "abc");
        assert_eq!(format_commit(""), "");
    }

    #[test]
    fn test_format_date() {
        assert_eq!(format_date("2026-06-22T12:34:56Z"), "2026-06-22");
        assert_eq!(format_date("2026-06-22"), "2026-06-22");
    }
}
