use anyhow::{bail, Result};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

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
    if let Some(date_part) = rfc3339.split('T').next() {
        date_part.to_string()
    } else {
        rfc3339.to_string()
    }
}

/// Write a live progress line to stderr (carriage-return + clear-line).
fn progress_line(label: &str) {
    let _ = write!(io::stderr(), "\r\x1B[K{}", label);
    let _ = io::stderr().flush();
}

/// Erase the current stderr line.
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

    let max_name_width = result
        .updated
        .iter()
        .map(|(n, _, _, _)| n.len())
        .chain(result.skipped.iter().map(|(n, _)| n.len()))
        .max()
        .unwrap_or(10);

    for (name, old_commit, new_commit, _date) in &result.updated {
        println!(
            "  {:<name_w$}  {:>7} -> {}",
            name,
            format_commit(old_commit),
            format_commit(new_commit),
            name_w = max_name_width,
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

/// Locate a skill's SKILL.md directory inside a cloned repo.
fn skill_repo_path(clone_dir: &Path, entry_path: &str) -> PathBuf {
    if entry_path.is_empty() {
        match normalize::detect_repo_type(clone_dir) {
            Ok(normalize::RepoType::SingleSkillAtRoot) => clone_dir.to_path_buf(),
            Ok(normalize::RepoType::SingleSkillInDir { name: _, path: p }) => p,
            _ => clone_dir.to_path_buf(),
        }
    } else {
        clone_dir.join(entry_path)
    }
}

pub fn execute(
    name: Option<&str>,
    verbose: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let mut manifest = Manifest::load(manifest_path)?;

    // --- Clean stale entries ---
    let stale = manifest.clean_stale(skills_dir);
    if !stale.is_empty() {
        eprintln!(
            "Skipping {} stale entries: {}",
            stale.len(),
            stale.join(", ")
        );
    }

    // --- Resolve which skills to update ---
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

    // --- Group by remote URL ---
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for skill_name in &skills_to_update {
        let remote = manifest.skills[skill_name].remote.clone();
        groups.entry(remote).or_default().push(skill_name.clone());
    }

    // Sort groups by skill count (desc) then URL for deterministic output
    let mut group_list: Vec<(String, Vec<String>)> = groups.into_iter().collect();
    group_list.sort_by(|a, b| {
        b.1.len()
            .cmp(&a.1.len())
            .then_with(|| a.0.cmp(&b.0))
    });

    let group_count = group_list.len();
    let mut result = UpdateResult {
        updated: Vec::new(),
        skipped: Vec::new(),
    };
    let mut update_date: Option<String> = None;

    for (group_idx, (remote, skill_names)) in group_list.iter().enumerate() {
        let label = format!(
            "Cloning {} ({} skill{})...",
            remote,
            skill_names.len(),
            if skill_names.len() == 1 { "" } else { "s" },
        );

        if verbose {
            println!("[{}/{}] {}", group_idx + 1, group_count, label);
        } else {
            progress_line(&label);
        }

        // --- Clone once per group ---
        let temp_dir = match fs::create_temp_dir() {
            Ok(d) => d,
            Err(e) => {
                clear_progress();
                eprintln!("Warning: Failed to create temp dir: {}", e);
                for skill_name in skill_names {
                    eprintln!("  Skipping {}", skill_name);
                }
                continue;
            }
        };

        if let Err(e) = git::clone_repo(remote, temp_dir.path()) {
            clear_progress();
            for skill_name in skill_names {
                eprintln!("Warning: Failed to update {}: {}", skill_name, e);
            }
            continue;
        }

        // Get the commit hash (same for all skills from this clone)
        let new_commit = git::get_commit_hash(temp_dir.path())
            .unwrap_or_else(|_| "unknown".to_string());
        let branch = git::get_branch(temp_dir.path())
            .unwrap_or_else(|_| "main".to_string());

        // Clear the "Cloning..." line from stderr (concise mode keeps stderr clean)
        if !verbose {
            clear_progress();
        }

        // --- Process each skill in the group ---
        for skill_name in skill_names {
            let entry = match manifest.skills.get(skill_name) {
                Some(e) => e,
                None => continue,
            };

            let old_commit = entry.commit.clone();
            let skill_path = skill_repo_path(temp_dir.path(), &entry.path);

            let frontmatter = match spec::validate_skill(&skill_path) {
                Ok(fm) => fm,
                Err(e) => {
                    eprintln!(
                        "Warning: Skipping {} — invalid updated SKILL.md: {}",
                        skill_name, e
                    );
                    continue;
                }
            };

            // Skip if no changes (compare first 7 chars handles short→full migration)
            if format_commit(&new_commit) == format_commit(&old_commit) {
                if verbose {
                    println!(
                        "  Already up to date: {} ({})",
                        skill_name,
                        format_commit(&new_commit),
                    );
                }
                result
                    .skipped
                    .push((skill_name.clone(), new_commit.clone()));
                continue;
            }

            let dest_dir = skills_dir.join(skill_name);
            if let Err(e) = normalize::copy_skill(&skill_path, &dest_dir) {
                eprintln!(
                    "Warning: Failed to copy updated files for {}: {}",
                    skill_name, e
                );
                continue;
            }

            if let Some(entry) = manifest.skills.get_mut(skill_name) {
                entry.update(&new_commit, &frontmatter.version.unwrap_or_default());
                entry.branch = branch.clone();
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
                }

                result.updated.push((
                    skill_name.clone(),
                    old_commit,
                    new_commit.clone(),
                    date_str,
                ));
            }
        }
    }

    // --- Save manifest ---
    manifest.save(manifest_path)?;

    // --- Final report on stdout ---
    if !verbose && (!result.updated.is_empty() || !result.skipped.is_empty()) {
        print_concise(&result);
    }

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

    fn create_git_repo_with_skill(dir: &Path, skill_name: &str, version: &str) -> String {
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

        let out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    // ── Tests ──

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
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        manifest.add_skill("ghost", make_entry("http://example.com/fake.git"));
        manifest.save(&manifest_path).unwrap();
        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_single_skill_success() {
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        let commit_hash = create_git_repo_with_skill(&repo_dir, "TestSkill", "2.0.0");
        let remote_url = format!("file://{}", repo_dir.display());
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
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
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        let mut entry = SkillEntry::new("test/repo", "", &make_fm("TestSkill"), &commit_hash, &remote_url);
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
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        let mut entry = SkillEntry::new("test/repo", "", &make_fm("TestSkill"), &commit_hash, &remote_url);
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
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("repo");
        let full_hash = create_git_repo_with_skill(&repo_dir, "TestSkill", "1.0.0");
        let short_hash: String = full_hash.chars().take(7).collect();
        let remote_url = format!("file://{}", repo_dir.display());
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        let mut entry = SkillEntry::new("test/repo", "", &make_fm("TestSkill"), &short_hash, &remote_url);
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
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
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
    fn test_update_grouped_remotes() {
        // Two skills from SAME remote: one changed, one unchanged
        let temp = TempDir::new().unwrap();
        let repo_dir = temp.path().join("multi-repo");
        fs::create_dir_all(repo_dir.join("skills/skill-b")).unwrap();
        fs::write(
            repo_dir.join("SKILL.md"),
            "---\nname: skill-a\ndescription: root skill\nversion: 1.0.0\n---\n",
        )
        .unwrap();
        fs::write(
            repo_dir.join("skills/skill-b/SKILL.md"),
            "---\nname: skill-b\ndescription: subdir skill\nversion: 2.0.0\n---\n",
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["add", "-A"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        let commit_hash = String::from_utf8(
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&repo_dir)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        let remote_url = format!("file://{}", repo_dir.display());

        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };

        let mut entry_a = SkillEntry::new(
            "test/repo",
            "",
            &SkillFrontmatter {
                name: "skill-a".into(),
                description: "root skill".into(),
                version: Some("1.0.0".into()),
                authors: None, tags: None, platforms: None, license: None, repository: None, docs: None,
            },
            &commit_hash,
            &remote_url,
        );
        entry_a.branch = "main".to_string();
        entry_a.version = "1.0.0".to_string();
        manifest.add_skill("skill-a", entry_a);

        let mut entry_b = SkillEntry::new(
            "test/repo",
            "skills/skill-b",
            &SkillFrontmatter {
                name: "skill-b".into(),
                description: "subdir skill".into(),
                version: Some("1.0.0".into()),
                authors: None, tags: None, platforms: None, license: None, repository: None, docs: None,
            },
            "oldhash0000000000000000000000000000000000",
            &remote_url,
        );
        entry_b.branch = "main".to_string();
        entry_b.version = "1.0.0".to_string();
        manifest.add_skill("skill-b", entry_b);
        manifest.save(&manifest_path).unwrap();

        for name in &["skill-a", "skill-b"] {
            let d = skills_dir.join(name);
            fs::create_dir_all(&d).unwrap();
            fs::write(
                d.join("SKILL.md"),
                format!("---\nname: {}\ndescription: old\nversion: 1.0.0\n---\n", name),
            )
            .unwrap();
        }

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
        let loaded = Manifest::load(&manifest_path).unwrap();

        let a = loaded.skills.get("skill-a").unwrap();
        assert_eq!(a.commit, commit_hash, "Unchanged skill keeps same hash");
        assert_eq!(a.version, "1.0.0");

        let b = loaded.skills.get("skill-b").unwrap();
        assert_eq!(b.commit, commit_hash, "Changed skill gets new hash");
        assert_eq!(b.version, "2.0.0");
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
