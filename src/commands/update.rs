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

#[allow(dead_code)]
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

/// Top border:  ╭── label ── ... ──╮  (padded to `width`)
fn box_top(label: &str, width: usize) -> String {
    // "╭─ " (3 visible) + label + " " + dashes + "─╮" (2 visible)
    let prefix = 3; // "╭─ "
    let suffix = 2; // "─╮"
    let inner = width.saturating_sub(prefix + suffix);
    let label_trunc: String = label.chars().take(inner.saturating_sub(1)).collect();
    let pad = inner.saturating_sub(label_trunc.len() + 1); // +1 for the space after label
    format!("╭─ {} {}─╮", label_trunc, line('─', pad),)
}

/// Bottom border:  ╰── ... ──╯  (padded to `width`)
fn box_bottom(width: usize) -> String {
    format!("╰{}╯", line('─', width.saturating_sub(2)))
}

/// A content line inside a box:  │  text + pad  │
///
/// `plain` is the visible text (no ANSI), `styled` may include ANSI codes.
fn box_line(plain: &str, styled: &str, width: usize) -> String {
    let inner = width.saturating_sub(6); // "│  " + "  │"
    let pad = inner.saturating_sub(plain.len());
    format!("│  {}{}  │", styled, " ".repeat(pad))
}

// ── per-group result printing ─────────────────────────────────────

#[derive(Debug, Default)]
struct GroupResult {
    updated: Vec<(String, String, String)>, // (name, old_commit, new_commit)
    skipped: Vec<(String, String)>,         // (name, commit)
    failed: Vec<(String, String)>,          // (name, reason)
}

impl GroupResult {
    fn is_empty(&self) -> bool {
        self.updated.is_empty() && self.skipped.is_empty() && self.failed.is_empty()
    }

    fn max_name_width(&self) -> usize {
        self.updated
            .iter()
            .map(|(n, _, _)| n.len())
            .chain(self.skipped.iter().map(|(n, _)| n.len()))
            .chain(self.failed.iter().map(|(n, _)| n.len()))
            .max()
            .unwrap_or(10)
    }

    fn print(&self) {
        if self.is_empty() {
            return;
        }
        let w = self.max_name_width();

        for (name, old_commit, new_commit) in &self.updated {
            println!(
                "  {} {:<name_w$}  {} → {}",
                style::styled(GREEN, "\u{2713}"),
                name,
                format_commit(old_commit),
                format_commit(new_commit),
                name_w = w,
            );
        }
        for (name, commit) in &self.skipped {
            println!(
                "  {} {:<name_w$}  up to date ({})",
                style::styled(DIM, "\u{2713}"),
                name,
                format_commit(commit),
                name_w = w,
            );
        }
        for (name, reason) in &self.failed {
            println!(
                "  {} {:<name_w$}  {}",
                style::styled(YELLOW, "\u{2717}"),
                name,
                reason,
                name_w = w,
            );
        }
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
    let header_plain = format!(
        "{} skill{} across {} repositor{}",
        total_skills,
        if total_skills == 1 { "" } else { "s" },
        total_repos,
        if total_repos == 1 { "y" } else { "ies" },
    );
    let header_styled = format!("{}{}{}", style::BOLD, header_plain, style::RESET);

    let term_width = 60usize;

    println!(
        "\n{}{}\n{}{}\n{}{}",
        CYAN,
        box_top("tome update", term_width),
        CYAN,
        box_line(&header_plain, &header_styled, term_width),
        CYAN,
        box_bottom(term_width),
    );
    println!("{}", style::RESET);

    // Phase 1: Clone all repos
    let group_count = group_list.len();
    let mut total_updated = 0usize;
    let mut total_skipped = 0usize;
    let mut total_failed = 0usize;
    let mut repo_results: Vec<(String, GroupResult)> = Vec::new();

    for (group_idx, (remote, skill_names)) in group_list.iter().enumerate() {
        let repo_label = remote
            .strip_prefix("https://github.com/")
            .unwrap_or(remote)
            .strip_suffix(".git")
            .unwrap_or(remote)
            .to_string();

        if group_count > 1 {
            eprintln!(
                "{} [{}/{}] Cloning {} ({} skill{})",
                style::styled(CYAN, "\u{25b8}"),
                group_idx + 1,
                group_count,
                repo_label,
                skill_names.len(),
                if skill_names.len() == 1 { "" } else { "s" },
            );
        } else {
            eprintln!("{} Cloning {}", style::styled(CYAN, "\u{25b8}"), repo_label,);
        }

        let temp_dir = match fs::create_temp_dir() {
            Ok(d) => d,
            Err(e) => {
                eprintln!(
                    "  {} Failed to create temp dir: {}",
                    style::styled(YELLOW, "\u{2717}"),
                    e
                );
                let mut gr = GroupResult::default();
                for sn in skill_names {
                    gr.failed.push((sn.clone(), "temp dir".to_string()));
                }
                total_failed += skill_names.len();
                repo_results.push((repo_label, gr));
                continue;
            }
        };

        if let Err(e) = git::clone_repo(remote, temp_dir.path()) {
            eprintln!(
                "  {} Clone failed: {}",
                style::styled(YELLOW, "\u{2717}"),
                e
            );
            let mut gr = GroupResult::default();
            for sn in skill_names {
                gr.failed.push((sn.clone(), "clone failed".to_string()));
            }
            total_failed += skill_names.len();
            repo_results.push((repo_label, gr));
            continue;
        }

        let new_commit =
            git::get_commit_hash(temp_dir.path()).unwrap_or_else(|_| "unknown".to_string());
        let branch = git::get_branch(temp_dir.path()).unwrap_or_else(|_| "main".to_string());

        let mut group_result = GroupResult::default();

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
                    if verbose {
                        eprintln!(
                            "  {} Skipping {} — invalid SKILL.md: {}",
                            style::styled(YELLOW, "\u{2717}"),
                            skill_name,
                            e
                        );
                    }
                    group_result
                        .failed
                        .push((skill_name.clone(), "invalid SKILL.md".to_string()));
                    continue;
                }
            };

            if format_commit(&new_commit) == format_commit(&old_commit) {
                if verbose {
                    println!(
                        "  {} Already up to date: {} ({})",
                        style::styled(DIM, "\u{2713}"),
                        skill_name,
                        format_commit(&new_commit)
                    );
                }
                group_result
                    .skipped
                    .push((skill_name.clone(), new_commit.clone()));
                continue;
            }

            let dest_dir = skills_dir.join(skill_name);
            if let Err(e) = normalize::copy_skill(&skill_path, &dest_dir) {
                if verbose {
                    eprintln!(
                        "  {} Failed to copy {}: {}",
                        style::styled(YELLOW, "\u{2717}"),
                        skill_name,
                        e
                    );
                }
                group_result
                    .failed
                    .push((skill_name.clone(), "copy failed".to_string()));
                continue;
            }

            if let Some(entry) = manifest.skills.get_mut(skill_name) {
                entry.update(&new_commit, &frontmatter.version.unwrap_or_default());
                entry.branch = branch.clone();
                entry.description = frontmatter.description;

                if verbose {
                    let ver = &entry.version;
                    if ver.is_empty() {
                        println!(
                            "  {} Updated: {} ({} -> {})",
                            style::styled(GREEN, "\u{2713}"),
                            skill_name,
                            format_commit(&old_commit),
                            format_commit(&new_commit)
                        );
                    } else {
                        println!(
                            "  {} Updated: {} {} ({} -> {})",
                            style::styled(GREEN, "\u{2713}"),
                            skill_name,
                            ver,
                            format_commit(&old_commit),
                            format_commit(&new_commit)
                        );
                    }
                }

                group_result
                    .updated
                    .push((skill_name.clone(), old_commit, new_commit.clone()));
            }
        }

        total_updated += group_result.updated.len();
        total_skipped += group_result.skipped.len();
        total_failed += group_result.failed.len();
        repo_results.push((repo_label, group_result));
    }

    // --- Save manifest ---
    manifest.save(manifest_path)?;

    // Phase 2: Print results grouped by repo
    let has_results = repo_results.iter().any(|(_, r)| !r.is_empty());
    if has_results && !verbose {
    println!();
        for (i, (repo_label, group_result)) in repo_results.iter().enumerate() {
            if group_result.is_empty() {
                continue;
            }
            if i > 0 {
                println!();
            }
            println!(
                "  {}{}{}",
                CYAN,
                style::styled(CYAN, repo_label),
                style::RESET,
            );
            group_result.print();
        }
        println!();
    } else if verbose && has_results {
        println!();
    }

    // --- Summary footer ---
    let mut parts: Vec<String> = Vec::new();
    if total_updated > 0 {
        parts.push(style::styled(GREEN, &format!("{} updated", total_updated)));
    }
    if total_skipped > 0 {
        parts.push(style::styled(DIM, &format!("{} up to date", total_skipped)));
    }
    if total_failed > 0 {
        parts.push(style::styled(YELLOW, &format!("{} failed", total_failed)));
    }

    let summary_plain = if parts.is_empty() {
        "nothing to update".to_string()
    } else {
        let plain_parts: Vec<String> = vec![
            if total_updated > 0 {
                format!("{} updated", total_updated)
            } else {
                String::new()
            },
            if total_skipped > 0 {
                format!("{} up to date", total_skipped)
            } else {
                String::new()
            },
            if total_failed > 0 {
                format!("{} failed", total_failed)
            } else {
                String::new()
            },
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
        plain_parts.join(" · ")
    };
    let summary_styled = parts.join(" · ");

    println!(
        "{}{}\n{}{}\n{}{}",
        CYAN,
        box_top("Result", term_width),
        CYAN,
        box_line(&summary_plain, &summary_styled, term_width),
        CYAN,
        box_bottom(term_width),
    );
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
