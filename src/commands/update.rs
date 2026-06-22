use anyhow::{Result, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::{git, manifest::Manifest, normalize, spec};
use crate::fs;
use crate::style::{self, CYAN, DIM, GREEN, YELLOW};

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

// ── box-drawing helpers ───────────────────────────────────────────

fn line(c: char, width: usize) -> String {
    std::iter::repeat_n(c, width).collect()
}

/// Top border of a box:  ╭── ... ──╮
fn box_top(label: &str, width: usize) -> String {
    let inner = width.saturating_sub(4); // "╭─ " + " ─╮"
    let label_trunc: String = label.chars().take(inner.saturating_sub(2)).collect();
    let pad = inner.saturating_sub(label_trunc.len() + 2); // +2 for "─ "
    format!(
        "╭─ {} {}{}─╮",
        label_trunc,
        line('─', pad),
        if label_trunc.len() < inner.saturating_sub(2) {
            "─"
        } else {
            ""
        }
    )
}

/// Bottom border:  ╰── ... ──╯
fn box_bottom(width: usize) -> String {
    format!("╰{}╯", line('─', width.saturating_sub(2)))
}

// ── result ────────────────────────────────────────────────────────

#[derive(Debug)]
struct UpdateResult {
    updated: Vec<(String, String, String, String)>, // (name, old_commit, new_commit, date)
    skipped: Vec<(String, String)>,                 // (name, commit)
    failed: Vec<(String, String)>,                  // (name, reason)
}

fn print_concise(result: &UpdateResult) {
    if result.updated.is_empty() && result.skipped.is_empty() && result.failed.is_empty() {
        return;
    }

    let max_name_width = result
        .updated
        .iter()
        .map(|(n, _, _, _)| n.len())
        .chain(result.skipped.iter().map(|(n, _)| n.len()))
        .chain(result.failed.iter().map(|(n, _)| n.len()))
        .max()
        .unwrap_or(10);

    // --- updated skills (green checkmark) ---
    for (name, old_commit, new_commit, _date) in &result.updated {
        println!(
            "  {} {:<name_w$}  {} -> {}",
            style::styled(GREEN, "\u{2713}"),
            name,
            format_commit(old_commit),
            format_commit(new_commit),
            name_w = max_name_width,
        );
    }

    // --- skipped skills (dimmed) ---
    for (name, commit) in &result.skipped {
        println!(
            "  {} {:<name_w$}  up to date ({})",
            style::styled(DIM, "\u{2713}"),
            name,
            format_commit(commit),
            name_w = max_name_width,
        );
    }

    // --- failed skills (yellow warning) ---
    for (name, reason) in &result.failed {
        println!(
            "  {} {:<name_w$}  {}",
            style::styled(YELLOW, "\u{2717}"),
            name,
            reason,
            name_w = max_name_width,
        );
    }
}

// ── repo path ─────────────────────────────────────────────────────

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

// ── execute ───────────────────────────────────────────────────────

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
            "{} Skipping {} stale entries: {}",
            style::styled(YELLOW, "\u{26a0}"),
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

    // Sort groups by skill count (desc) then URL
    let mut group_list: Vec<(String, Vec<String>)> = groups.into_iter().collect();
    group_list.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));

    // --- Header ---
    let total_skills: usize = group_list.iter().map(|(_, v)| v.len()).sum();
    let total_repos = group_list.len();
    let header = format!(
        "{} skill{} across {} repositor{}",
        total_skills,
        if total_skills == 1 { "" } else { "s" },
        total_repos,
        if total_repos == 1 { "y" } else { "ies" },
    );

    let term_width = 60usize; // conservative width that looks good
    let top = box_top("tome update", term_width);
    let mid = format!("│  {}{}{}  │", style::BOLD, header, style::RESET);
    let bot = box_bottom(term_width);

    println!("\n{}{}\n{}{}\n{}", CYAN, top, CYAN, mid, CYAN);
    println!("{}{}", CYAN, bot);
    println!("{}", style::RESET);

    // --- Process groups ---
    let group_count = group_list.len();
    let mut result = UpdateResult {
        updated: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
    };
    let mut update_date: Option<String> = None;

    for (group_idx, (remote, skill_names)) in group_list.iter().enumerate() {
        let repo_label = remote
            .strip_prefix("https://github.com/")
            .unwrap_or(remote)
            .strip_suffix(".git")
            .unwrap_or(remote);

        // Print group header
        if group_count > 1 {
            println!(
                "\n{} [{}/{}] {}Cloning{} {} ({} skill{})",
                style::styled(CYAN, "\u{25b8}"),
                group_idx + 1,
                group_count,
                style::BOLD,
                style::RESET,
                repo_label,
                skill_names.len(),
                if skill_names.len() == 1 { "" } else { "s" },
            );
        } else {
            println!(
                "\n{} {}Cloning{} {} ({} skill{})",
                style::styled(CYAN, "\u{25b8}"),
                style::BOLD,
                style::RESET,
                repo_label,
                skill_names.len(),
                if skill_names.len() == 1 { "" } else { "s" },
            );
        }

        // --- Clone ---
        let temp_dir = match fs::create_temp_dir() {
            Ok(d) => d,
            Err(e) => {
                eprintln!(
                    "  {} Failed to create temp dir: {}",
                    style::styled(YELLOW, "\u{2717}"),
                    e,
                );
                continue;
            }
        };

        if let Err(e) = git::clone_repo(remote, temp_dir.path()) {
            eprintln!(
                "  {} Clone failed: {}",
                style::styled(YELLOW, "\u{2717}"),
                e,
            );
            continue;
        }

        // Get the commit hash
        let new_commit =
            git::get_commit_hash(temp_dir.path()).unwrap_or_else(|_| "unknown".to_string());
        let branch = git::get_branch(temp_dir.path()).unwrap_or_else(|_| "main".to_string());

        // --- Process each skill ---
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
                        "  {} Skipping {} — invalid SKILL.md: {}",
                        style::styled(YELLOW, "\u{2717}"),
                        skill_name,
                        e,
                    );
                    result
                        .failed
                        .push((skill_name.clone(), "invalid SKILL.md".to_string()));
                    continue;
                }
            };

            // Skip if no changes
            if format_commit(&new_commit) == format_commit(&old_commit) {
                if verbose {
                    println!(
                        "  {} Already up to date: {} ({})",
                        style::styled(DIM, "\u{2713}"),
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
                    "  {} Failed to copy {}: {}",
                    style::styled(YELLOW, "\u{2717}"),
                    skill_name,
                    e,
                );
                result
                    .failed
                    .push((skill_name.clone(), "copy failed".to_string()));
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
                            "  {} Updated: {} ({} -> {})",
                            style::styled(GREEN, "\u{2713}"),
                            skill_name,
                            format_commit(&old_commit),
                            format_commit(&new_commit),
                        );
                    } else {
                        println!(
                            "  {} Updated: {} {} ({} -> {})",
                            style::styled(GREEN, "\u{2713}"),
                            skill_name,
                            ver,
                            format_commit(&old_commit),
                            format_commit(&new_commit),
                        );
                    }
                }

                result
                    .updated
                    .push((skill_name.clone(), old_commit, new_commit.clone(), date_str));
            }
        }
    }

    // --- Save manifest ---
    manifest.save(manifest_path)?;

    // --- Final report ---
    if !verbose
        && (!result.updated.is_empty() || !result.skipped.is_empty() || !result.failed.is_empty())
    {
        println!();
        print_concise(&result);
    }

    println!();

    // --- Summary footer ---
    let mut parts: Vec<String> = Vec::new();
    if !result.updated.is_empty() {
        parts.push(style::styled(
            GREEN,
            &format!("{} updated", result.updated.len()),
        ));
    }
    if !result.skipped.is_empty() {
        parts.push(style::styled(
            DIM,
            &format!("{} up to date", result.skipped.len()),
        ));
    }
    if !result.failed.is_empty() {
        parts.push(style::styled(
            YELLOW,
            &format!("{} failed", result.failed.len()),
        ));
    }

    let summary = if parts.is_empty() {
        "nothing to update".to_string()
    } else {
        parts.join(" · ")
    };

    let footer_line = format!("│  {}{}{}  │", style::BOLD, summary, style::RESET,);
    let footer_bot = box_bottom(term_width);

    println!("{}{}", CYAN, box_top("Result", term_width));
    println!("{}{}", CYAN, footer_line);
    println!("{}{}", CYAN, footer_bot);
    println!("{}", style::RESET);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::{Manifest, SkillEntry};
    use crate::core::spec::SkillFrontmatter;
    use clap::Parser;
    use std::collections::HashMap;
    use std::fs;
    use std::process::Command;
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

    fn make_entry(name: &str, commit: &str, remote: &str) -> (String, SkillEntry) {
        let fm = make_fm(name);
        (
            name.to_string(),
            SkillEntry::new(&format!("owner/{}", name), "", &fm, commit, remote),
        )
    }

    /// Create a git repository on disk with a single SKILL.md and return its file:// URL.
    fn create_git_repo_with_skill(temp: &TempDir, skill_content: &str) -> String {
        let repo_dir = temp.path().join("repo");
        fs::create_dir_all(&repo_dir).unwrap();
        fs::write(repo_dir.join("SKILL.md"), skill_content).unwrap();

        Command::new("git")
            .args(["init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo_dir)
            .output()
            .unwrap();

        format!("file://{}", repo_dir.display())
    }

    // ── tests ─────────────────────────────────────────────────────

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
    }

    #[test]
    fn test_update_stale_entries_cleaned() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create a real skill dir on disk
        fs::create_dir_all(skills_dir.join("real")).unwrap();
        fs::write(
            skills_dir.join("real/SKILL.md"),
            "---\nname: real\ndescription: r\n---\n",
        )
        .unwrap();

        // Manifest has "real" and "ghost"
        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill(
            "real",
            make_entry("real", "abc1234", "https://example.com/repo.git").1,
        );
        manifest.add_skill(
            "ghost",
            make_entry("ghost", "def5678", "https://example.com/repo.git").1,
        );
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_err() || result.is_ok()); // may fail on clone, but stale entry should be cleaned

        let loaded = Manifest::load(&manifest_path).unwrap();
        assert!(!loaded.skills.contains_key("ghost"));
    }

    #[test]
    fn test_update_single_skill_success() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = create_git_repo_with_skill(
            &temp,
            "---\nname: my-skill\ndescription: test\nversion: 1.0.0\n---\n# Hello\n",
        );

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("my-skill", make_entry("my-skill", "0000000", &url).1);
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        // Install the skill first (simulate previous `add`)
        let clone_temp = TempDir::new().unwrap();
        let _ = git::clone_repo_quiet(&url, clone_temp.path());
        // Find the SKILL.md and copy it
        let fm = spec::validate_skill(clone_temp.path()).unwrap();
        let dest = skills_dir.join(&fm.name);
        normalize::copy_skill(clone_temp.path(), &dest).unwrap();

        let result = execute(Some("my-skill"), false, &skills_dir, &manifest_path);
        // Should succeed (skill is up to date)
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_skips_when_no_changes() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = create_git_repo_with_skill(&temp, "---\nname: skip-skill\ndescription: t\n---\n");

        // Get the real commit hash
        let clone_temp = TempDir::new().unwrap();
        git::clone_repo_quiet(&url, clone_temp.path()).unwrap();
        let real_commit = git::get_commit_hash(clone_temp.path()).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("skip-skill", make_entry("skip-skill", &real_commit, &url).1);
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        // Install the skill
        let fm = spec::validate_skill(clone_temp.path()).unwrap();
        let dest = skills_dir.join(&fm.name);
        normalize::copy_skill(clone_temp.path(), &dest).unwrap();

        let result = execute(Some("skip-skill"), false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        // Verify no update occurred
        let loaded = Manifest::load(&manifest_path).unwrap();
        let entry = loaded.skills.get("skip-skill").unwrap();
        assert_eq!(entry.commit, real_commit);
    }

    #[test]
    fn test_update_skips_when_no_changes_verbose() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = create_git_repo_with_skill(&temp, "---\nname: verb-skip\ndescription: t\n---\n");

        let clone_temp = TempDir::new().unwrap();
        git::clone_repo_quiet(&url, clone_temp.path()).unwrap();
        let real_commit = git::get_commit_hash(clone_temp.path()).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("verb-skip", make_entry("verb-skip", &real_commit, &url).1);
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        let fm = spec::validate_skill(clone_temp.path()).unwrap();
        let dest = skills_dir.join(&fm.name);
        normalize::copy_skill(clone_temp.path(), &dest).unwrap();

        let result = execute(Some("verb-skip"), true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_skips_when_manifest_has_short_hash() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = create_git_repo_with_skill(&temp, "---\nname: short-hash\ndescription: t\n---\n");

        let clone_temp = TempDir::new().unwrap();
        git::clone_repo_quiet(&url, clone_temp.path()).unwrap();
        let real_commit = git::get_commit_hash(clone_temp.path()).unwrap();
        let short_hash = &real_commit[..7];

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("short-hash", make_entry("short-hash", short_hash, &url).1);
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        let fm = spec::validate_skill(clone_temp.path()).unwrap();
        let dest = skills_dir.join(&fm.name);
        normalize::copy_skill(clone_temp.path(), &dest).unwrap();

        let result = execute(Some("short-hash"), false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_respects_changed_commit_hash() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = create_git_repo_with_skill(&temp, "---\nname: changed\ndescription: t\n---\n");

        let clone_temp = TempDir::new().unwrap();
        git::clone_repo_quiet(&url, clone_temp.path()).unwrap();
        let real_commit = git::get_commit_hash(clone_temp.path()).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        // Use a different fake commit hash so it appears outdated
        manifest.add_skill(
            "changed",
            make_entry("changed", "0000000000000000000000000000000000000000", &url).1,
        );
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        let fm = spec::validate_skill(clone_temp.path()).unwrap();
        let dest = skills_dir.join(&fm.name);
        normalize::copy_skill(clone_temp.path(), &dest).unwrap();

        let result = execute(Some("changed"), false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        // Manifest should now have the real commit
        let loaded = Manifest::load(&manifest_path).unwrap();
        let entry = loaded.skills.get("changed").unwrap();
        assert_eq!(entry.commit, real_commit);
    }

    #[test]
    fn test_update_grouped_remotes() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let url = create_git_repo_with_skill(
            &temp,
            "---\nname: group-skill-a\ndescription: a\n---\n# A\n",
        );

        let clone_temp = TempDir::new().unwrap();
        git::clone_repo_quiet(&url, clone_temp.path()).unwrap();
        let real_commit = git::get_commit_hash(clone_temp.path()).unwrap();

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
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        // Install skill-a (validate that the clone is well-formed)
        spec::validate_skill(clone_temp.path()).unwrap();
        let dest_a = skills_dir.join("group-skill-a");
        normalize::copy_skill(clone_temp.path(), &dest_a).unwrap();
        // Install skill-b (same clone)
        let dest_b = skills_dir.join("group-skill-b");
        normalize::copy_skill(clone_temp.path(), &dest_b).unwrap();

        let result = execute(None, false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        let loaded = Manifest::load(&manifest_path).unwrap();
        // Both should now have the real commit since they share the remote
        assert_eq!(
            loaded.skills.get("group-skill-a").unwrap().commit,
            real_commit
        );
        assert_eq!(
            loaded.skills.get("group-skill-b").unwrap().commit,
            real_commit
        );
    }

    #[test]
    fn test_update_verbose_flag_accepted() {
        let args: Vec<&str> = vec!["tome", "update", "--verbose"];
        let cmd = crate::config::Cli::try_parse_from(args);
        assert!(cmd.is_ok());
    }

    #[test]
    fn test_format_commit() {
        assert_eq!(format_commit("abc1234567890abcdef"), "abc1234");
        assert_eq!(format_commit("abc"), "abc");
    }

    #[test]
    fn test_format_date() {
        assert_eq!(format_date("2026-06-22T12:34:56Z"), "2026-06-22");
        assert_eq!(format_date("just-a-date"), "just-a-date");
    }
}
