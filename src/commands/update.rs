use anyhow::{bail, Result};
use std::io::{self, Write};
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

/// Write a live progress line to stderr (carriage-return + clear-line,
/// so the same line gets overwritten on each call).
fn progress_line(current: usize, total: usize, label: &str) {
    // \r     = move cursor to column 0
    // \x1B[K = clear from cursor to end of line
    let _ = write!(io::stderr(), "\r\x1B[K[{}/{}] {}", current, total, label);
    let _ = io::stderr().flush();
}

/// Erase the last progress line from stderr.
fn clear_progress() {
    let _ = write!(io::stderr(), "\r\x1B[K");
    let _ = io::stderr().flush();
}

#[derive(Debug)]
struct UpdateResult {
    updated: Vec<(String, String, String, String)>, // (name, old_commit, new_commit, date)
    skipped: Vec<(String, String)>,                 // (name, commit)
}

fn print_concise(result: &UpdateResult) {
    if result.updated.is_empty() && result.skipped.is_empty() {
        return;
    }

    // Find column widths for alignment
    let max_name_width = result
        .updated
        .iter()
        .map(|(n, _, _, _)| n.len())
        .chain(result.skipped.iter().map(|(n, _)| n.len()))
        .max()
        .unwrap_or(10);
    let commit_width = 7;

    for (name, old_commit, new_commit, _date) in &result.updated {
        println!(
            "  {:<name_w$}  {:>commit_w$} -> {}",
            name,
            format_commit(old_commit),
            format_commit(new_commit),
            name_w = max_name_width,
            commit_w = commit_width,
        );
    }

    for (name, commit) in &result.skipped {
        println!(
            "  {:<name_w$}  up to date ({})",
            name,
            format_commit(commit),
            name_w = max_name_width,
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

    let total = skills_to_update.len();
    let mut result = UpdateResult {
        updated: Vec::new(),
        skipped: Vec::new(),
    };
    let mut update_date: Option<String> = None;

    for (i, skill_name) in skills_to_update.iter().enumerate() {
        let entry = manifest.skills.get(skill_name).unwrap();
        let remote = entry.remote.clone();
        let old_commit = entry.commit.clone();

        let temp_dir = fs::create_temp_dir()?;

        // --- Progress display ---
        if verbose {
            println!("[{}/{}] Updating {} from {}...", i + 1, total, skill_name, remote);
        } else {
            progress_line(i + 1, total, &format!("Fetching {}...", skill_name));
        }

        // --- Clone ---
        if let Err(e) = git::clone_repo(&remote, temp_dir.path()) {
            if !verbose {
                clear_progress();
            }
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
                if !verbose {
                    clear_progress();
                }
                eprintln!("Warning: Skipping {} — invalid updated SKILL.md: {}", skill_name, e);
                continue;
            }
        };

        let new_commit = git::get_commit_hash(temp_dir.path())
            .unwrap_or_else(|_| "unknown".to_string());

        // --- Skip if no changes (compare first 7 chars to handle
        //     migration from short to full hashes) ---
        if format_commit(&new_commit) == format_commit(&old_commit) {
            if verbose {
                println!("  Already up to date: {} ({})", skill_name, format_commit(&new_commit));
            } else {
                // Overwrite progress with skip notice, then newline
                progress_line(i + 1, total, &format!("{} up to date ({})", skill_name, format_commit(&new_commit)));
                let _ = writeln!(io::stderr());
            }
            result.skipped.push((skill_name.clone(), new_commit));
            continue;
        }

        let branch = git::get_branch(temp_dir.path())
            .unwrap_or_else(|_| "main".to_string());

        let dest_dir = skills_dir.join(skill_name);
        if let Err(e) = normalize::copy_skill(&skill_repo_path, &dest_dir) {
            if !verbose {
                clear_progress();
            }
            eprintln!("Warning: Failed to copy updated files for {}: {}", skill_name, e);
            continue;
        }

        if let Some(entry) = manifest.skills.get_mut(skill_name) {
            entry.update(&new_commit, &frontmatter.version.unwrap_or_default());
            entry.branch = branch;
            entry.description = frontmatter.description;

            let date_str = format_date(&entry.updated);
            if update_date.is_none() {
                update_date = Some(date_str.clone());
            }

            if verbose {
                let ver = &entry.version;
                if ver.is_empty() {
                    println!(
                        "  Updated: {} ({} -> {})",
                        skill_name,
                        format_commit(&old_commit),
                        format_commit(&new_commit),
                    );
                } else {
                    println!(
                        "  Updated: {} {} ({} -> {})",
                        skill_name,
                        ver,
                        format_commit(&old_commit),
                        format_commit(&new_commit),
                    );
                }
            } else {
                // Overwrite progress with result, then newline
                let old_str = format_commit(&old_commit);
                let new_str = format_commit(&new_commit);
                progress_line(
                    i + 1,
                    total,
                    &format!("{} {} -> {}", skill_name, old_str, new_str),
                );
                let _ = writeln!(io::stderr());
            }

            result.updated.push((
                skill_name.clone(),
                old_commit,
                new_commit,
                date_str,
            ));
        }
    }

    manifest.save(manifest_path)?;

    // Clear progress line (stderr) so it doesn't linger before the final report
    if !verbose {
        clear_progress();
    }

    if !verbose && !result.updated.is_empty() || !result.skipped.is_empty() {
        print_concise(&result);
    }

    // Build summary line
    let date_suffix = update_date
        .map(|d| format!(" at {}", d))
        .unwrap_or_default();

    let total = result.updated.len() + result.skipped.len();
    if result.updated.is_empty() && !result.skipped.is_empty() {
        println!(
            "Update complete: all {} skill(s) already up to date{}.",
            total,
            date_suffix,
        );
    } else {
        print!(
            "Update complete. {} skill(s) updated",
            result.updated.len(),
        );
        if !result.skipped.is_empty() {
            print!(", {} already up to date", result.skipped.len());
        }
        println!("{}.", date_suffix);
    }

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

    fn create_git_repo_with_skill(
        dir: &Path,
        skill_name: &str,
        version: &str,
    ) -> String {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!(
                "---\nname: {}\ndescription: test\nversion: {}\n---\n",
                skill_name, version,
            ),
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(dir)
            .output()
            .unwrap();

        // Return the commit hash
        let out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
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

        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
        manifest.add_skill("ghost", make_entry("http://example.com/fake.git"));
        manifest.save(&manifest_path).unwrap();

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_with_local_repo_success() {
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        let commit_hash = create_git_repo_with_skill(&repo_dir, "TestSkill", "2.0.0");
        let remote_url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
        let mut entry =
            SkillEntry::new("test/repo", "", &make_fm("TestSkill"), "oldhash", &remote_url);
        entry.branch = "main".to_string();
        manifest.add_skill("TestSkill", entry);
        manifest.save(&manifest_path).unwrap();

        fs::create_dir_all(skills_dir.join("TestSkill")).unwrap();
        fs::write(
            skills_dir.join("TestSkill").join("SKILL.md"),
            "---\nname: TestSkill\ndescription: old\nversion: 1.0.0\n---\n",
        )
        .unwrap();

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        let loaded = Manifest::load(&manifest_path).unwrap();
        let entry = loaded.skills.get("TestSkill").unwrap();
        assert_eq!(entry.version, "2.0.0");
        assert_eq!(entry.commit, commit_hash);
    }

    #[test]
    fn test_update_skips_when_no_changes() {
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        let commit_hash = create_git_repo_with_skill(&repo_dir, "TestSkill", "1.0.0");
        let remote_url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        // The manifest ALREADY has the correct commit hash
        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
        let mut entry = SkillEntry::new(
            "test/repo",
            "",
            &make_fm("TestSkill"),
            &commit_hash, // same as repo HEAD
            &remote_url,
        );
        entry.branch = "main".to_string();
        entry.version = "1.0.0".to_string();
        manifest.add_skill("TestSkill", entry);
        manifest.save(&manifest_path).unwrap();

        // Create skill dir so it's not treated as stale
        fs::create_dir_all(skills_dir.join("TestSkill")).unwrap();
        fs::write(
            skills_dir.join("TestSkill").join("SKILL.md"),
            "---\nname: TestSkill\ndescription: test\nversion: 1.0.0\n---\n",
        )
        .unwrap();

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        // Manifest should NOT be changed (commit stays the same, updated timestamp unchanged)
        let loaded = Manifest::load(&manifest_path).unwrap();
        let entry = loaded.skills.get("TestSkill").unwrap();
        assert_eq!(entry.commit, commit_hash);
        assert_eq!(entry.version, "1.0.0");
    }

    #[test]
    fn test_update_skips_when_no_changes_verbose() {
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        let commit_hash = create_git_repo_with_skill(&repo_dir, "TestSkill", "1.0.0");
        let remote_url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
        let mut entry = SkillEntry::new(
            "test/repo",
            "",
            &make_fm("TestSkill"),
            &commit_hash,
            &remote_url,
        );
        entry.branch = "main".to_string();
        entry.version = "1.0.0".to_string();
        manifest.add_skill("TestSkill", entry);
        manifest.save(&manifest_path).unwrap();

        fs::create_dir_all(skills_dir.join("TestSkill")).unwrap();
        fs::write(
            skills_dir.join("TestSkill").join("SKILL.md"),
            "---\nname: TestSkill\ndescription: test\nversion: 1.0.0\n---\n",
        )
        .unwrap();

        let result = execute(None, true, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        let loaded = Manifest::load(&manifest_path).unwrap();
        let entry = loaded.skills.get("TestSkill").unwrap();
        assert_eq!(entry.commit, commit_hash);
    }

    #[test]
    fn test_update_skips_when_manifest_has_short_hash() {
        // Simulate user's real scenario: manifest has short (7-char) hash
        // from the original `tome add`, while git returns full (40-char) hash.
        // The skip logic must compare by first 7 chars only.
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        let full_hash = create_git_repo_with_skill(&repo_dir, "TestSkill", "1.0.0");
        let short_hash: String = full_hash.chars().take(7).collect();
        let remote_url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
        let mut entry = SkillEntry::new(
            "test/repo",
            "",
            &make_fm("TestSkill"),
            &short_hash,
            &remote_url,
        );
        entry.branch = "main".to_string();
        entry.version = "1.0.0".to_string();
        manifest.add_skill("TestSkill", entry);
        manifest.save(&manifest_path).unwrap();

        fs::create_dir_all(skills_dir.join("TestSkill")).unwrap();
        fs::write(
            skills_dir.join("TestSkill").join("SKILL.md"),
            "---\nname: TestSkill\ndescription: test\nversion: 1.0.0\n---\n",
        )
        .unwrap();

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        let loaded = Manifest::load(&manifest_path).unwrap();
        let entry = loaded.skills.get("TestSkill").unwrap();
        assert_eq!(entry.commit, short_hash, "Short hash should be preserved");
    }

    #[test]
    fn test_update_respects_changed_commit_hash() {
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        let actual_hash = create_git_repo_with_skill(&repo_dir, "TestSkill", "2.0.0");
        let remote_url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
        // Different hash in manifest
        let mut entry = SkillEntry::new(
            "test/repo",
            "",
            &make_fm("TestSkill"),
            "fffffff000000000000000000000000000000000",
            &remote_url,
        );
        entry.branch = "main".to_string();
        entry.version = "1.0.0".to_string();
        manifest.add_skill("TestSkill", entry);
        manifest.save(&manifest_path).unwrap();

        fs::create_dir_all(skills_dir.join("TestSkill")).unwrap();
        fs::write(
            skills_dir.join("TestSkill").join("SKILL.md"),
            "---\nname: TestSkill\ndescription: old\nversion: 1.0.0\n---\n",
        )
        .unwrap();

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        let loaded = Manifest::load(&manifest_path).unwrap();
        let entry = loaded.skills.get("TestSkill").unwrap();
        assert_eq!(entry.commit, actual_hash);
        assert_eq!(entry.version, "2.0.0");
    }

    #[test]
    fn test_update_mixed_updated_and_skipped() {
        // Two skills: one changed, one unchanged
        let temp = TempDir::new().unwrap();

        // --- Changed skill ---
        let repo_a = temp.path().join("repo-a");
        let hash_a = create_git_repo_with_skill(&repo_a, "ChangedSkill", "2.0.0");
        let remote_a = format!("file://{}", repo_a.display());

        // --- Unchanged skill ---
        let repo_b = temp.path().join("repo-b");
        let hash_b = create_git_repo_with_skill(&repo_b, "UnchangedSkill", "1.0.0");
        let remote_b = format!("file://{}", repo_b.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };

        // Changed — manifest has old hash
        let mut entry_a = SkillEntry::new(
            "test/repo",
            "",
            &make_fm("ChangedSkill"),
            "oldhash111111111111111111111111111111111",
            &remote_a,
        );
        entry_a.branch = "main".to_string();
        entry_a.version = "1.0.0".to_string();
        manifest.add_skill("ChangedSkill", entry_a);

        // Unchanged — manifest has current hash
        let mut entry_b = SkillEntry::new(
            "test/repo",
            "",
            &make_fm("UnchangedSkill"),
            &hash_b,
            &remote_b,
        );
        entry_b.branch = "main".to_string();
        entry_b.version = "1.0.0".to_string();
        manifest.add_skill("UnchangedSkill", entry_b);
        manifest.save(&manifest_path).unwrap();

        for name in &["ChangedSkill", "UnchangedSkill"] {
            fs::create_dir_all(skills_dir.join(name)).unwrap();
            fs::write(
                skills_dir.join(name).join("SKILL.md"),
                format!("---\nname: {}\ndescription: test\nversion: 1.0.0\n---\n", name),
            )
            .unwrap();
        }

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        let loaded = Manifest::load(&manifest_path).unwrap();

        let changed = loaded.skills.get("ChangedSkill").unwrap();
        assert_eq!(changed.commit, hash_a, "Changed skill should have new hash");
        assert_eq!(changed.version, "2.0.0");

        let unchanged = loaded.skills.get("UnchangedSkill").unwrap();
        assert_eq!(unchanged.commit, hash_b, "Unchanged skill should keep same hash");
        assert_eq!(unchanged.version, "1.0.0");
    }

    #[test]
    fn test_update_verbose_flag_accepted() {
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
