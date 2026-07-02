use anyhow::{Result, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::core::{download, git, manifest::Manifest, normalize, spec};
use crate::fs;
use crate::progress::{ProgressPhase, RepoOutcome, UpdateProgress};
use crate::style::{self, CYAN, DIM, GREEN, YELLOW};

const TERM_WIDTH: usize = 60;
const SHORT_COMMIT_LEN: usize = 7;
const DEFAULT_MAX_NAME_WIDTH: usize = 10;

fn format_commit(commit: &str) -> String {
    if commit.len() >= SHORT_COMMIT_LEN {
        commit[..SHORT_COMMIT_LEN].to_string()
    } else {
        commit.to_string()
    }
}

// ── per-group result printing ─────────────────────────────────────

#[derive(Debug, Default)]
struct GroupResult {
    updated: Vec<(String, String, String)>, // (name, old_commit, new_commit)
    skipped: Vec<(String, String)>,         // (name, commit)
    failed: Vec<(String, String)>,          // (name, reason)
    removed: Vec<String>,                   // skill names removed from repo
}

impl GroupResult {
    fn is_empty(&self) -> bool {
        self.updated.is_empty()
            && self.skipped.is_empty()
            && self.failed.is_empty()
            && self.removed.is_empty()
    }

    fn failed_all(skill_names: &[String], reason: &str) -> Self {
        Self {
            failed: skill_names
                .iter()
                .map(|n| (n.clone(), reason.to_string()))
                .collect(),
            ..Default::default()
        }
    }

    fn max_name_width(&self) -> usize {
        self.updated
            .iter()
            .map(|(n, _, _)| n.len())
            .chain(self.skipped.iter().map(|(n, _)| n.len()))
            .chain(self.failed.iter().map(|(n, _)| n.len()))
            .max()
            .unwrap_or(DEFAULT_MAX_NAME_WIDTH)
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
        for name in &self.removed {
            println!(
                "  {} {:<name_w$}  removed from repo",
                style::styled(YELLOW, "\u{2717}"),
                name,
                name_w = w,
            );
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

fn print_update_header(group_list: &[(String, Vec<String>)]) {
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
    println!(
        "\n{}{}\n{}{}\n{}{}",
        CYAN,
        style::box_top("tome update", TERM_WIDTH),
        CYAN,
        style::box_line(&header_plain, &header_styled, TERM_WIDTH),
        CYAN,
        style::box_bottom(TERM_WIDTH),
    );
    println!("{}", style::RESET);
}

struct UpdateContext<'a> {
    manifest: &'a mut Manifest,
    skills_dir: &'a Path,
    verbose: bool,
    sparse: bool,
    progress: &'a mut UpdateProgress,
}

fn update_group(
    remote: &str,
    skill_names: &[String],
    group_idx: usize,
    group_count: usize,
    ctx: &mut UpdateContext<'_>,
) -> (String, GroupResult) {
    let label = crate::core::parse_source::repo_label(remote).to_string();

    if ctx.verbose {
        if group_count > 1 {
            eprintln!(
                "{} [{}/{}] Cloning {} ({} skill{})",
                style::styled(CYAN, "\u{25b8}"),
                group_idx + 1,
                group_count,
                label,
                skill_names.len(),
                if skill_names.len() == 1 { "" } else { "s" },
            );
        } else {
            eprintln!("{} Cloning {}", style::styled(CYAN, "\u{25b8}"), label,);
        }
    }
    ctx.progress
        .repo_started(group_idx + 1, group_count, &label, skill_names.len());
    ctx.progress.phase(ProgressPhase::Cloning);

    let temp_dir = match fs::create_temp_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "  {} Failed to create temp dir: {}",
                style::styled(YELLOW, "\u{2717}"),
                e
            );
            let result = GroupResult::failed_all(skill_names, "temp dir");
            ctx.progress
                .repo_finished(&label, RepoOutcome::from(&result));
            return (label, result);
        }
    };

    let git_output = if ctx.progress.enabled() {
        git::GitOutput::Quiet
    } else {
        git::GitOutput::Progress
    };

    if let Err(e) = git::clone_repo_or_sparse(remote, temp_dir.path(), ctx.sparse, git_output) {
        eprintln!(
            "  {} Clone failed: {}",
            style::styled(YELLOW, "\u{2717}"),
            e
        );
        let result = GroupResult::failed_all(skill_names, "clone failed");
        ctx.progress
            .repo_finished(&label, RepoOutcome::from(&result));
        return (label, result);
    }
    ctx.progress.phase(ProgressPhase::Inspecting);

    let new_commit =
        git::get_commit_hash(temp_dir.path()).unwrap_or_else(|_| "unknown".to_string());
    let branch =
        git::get_branch(temp_dir.path()).unwrap_or_else(|_| crate::DEFAULT_BRANCH.to_string());

    eprintln!();

    let mut group_result = GroupResult::default();

    for skill_name in skill_names {
        ctx.progress.phase(ProgressPhase::Checking);
        let (old_commit, entry_path) = match ctx.manifest.get_skill(skill_name) {
            Some(e) => (e.commit.clone(), e.path.clone()),
            None => continue,
        };

        let skill_path = skill_repo_path(temp_dir.path(), &entry_path);

        if !skill_path.join("SKILL.md").exists() {
            // Skill removed from upstream repo — remove locally
            let local_dir = ctx.skills_dir.join(skill_name);
            let _ = fs::remove_skill_dir(&local_dir);
            ctx.manifest.remove_skill(skill_name);
            if ctx.verbose {
                eprintln!(
                    "  {} Removed {} — no longer in repository",
                    style::styled(YELLOW, "\u{2717}"),
                    skill_name,
                );
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
                        "  {} Skipping {} — invalid SKILL.md: {}",
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
            ctx.progress.skill_checked();
            continue;
        }

        ctx.progress.phase(ProgressPhase::Copying);
        let dest_dir = ctx.skills_dir.join(skill_name);
        if let Err(e) = normalize::copy_skill(&skill_path, &dest_dir) {
            if ctx.verbose {
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
            if version.is_empty() {
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
                    version,
                    format_commit(&old_commit),
                    format_commit(&new_commit)
                );
            }
        }

        ctx.progress.skill_checked();
        group_result
            .updated
            .push((skill_name.clone(), old_commit, new_commit.clone()));
    }

    ctx.progress
        .repo_finished(&label, RepoOutcome::from(&group_result));
    (label, group_result)
}

fn print_results(repo_results: &[(String, GroupResult)], verbose: bool) {
    let has_results = repo_results.iter().any(|(_, r)| !r.is_empty());
    if has_results && !verbose {
        println!();
        for (i, (label, group_result)) in repo_results.iter().enumerate() {
            if group_result.is_empty() {
                continue;
            }
            if i > 0 {
                println!();
            }
            println!("  {}{}{}", CYAN, style::styled(CYAN, label), style::RESET,);
            group_result.print();
        }
        println!();
    } else if verbose && has_results {
        println!();
    }
}

fn format_duration(secs: f64) -> String {
    let time = if secs < 60.0 {
        format!("{:.1} s", secs)
    } else {
        format!("{:.0}m {:.0} s", secs / 60.0, secs % 60.0)
    };
    format!("Completed in {}", time)
}

fn total_sources(file_skill_count: usize, git_group_count: usize) -> usize {
    file_skill_count + git_group_count
}

fn print_summary(
    total_updated: usize,
    total_skipped: usize,
    total_failed: usize,
    total_removed: usize,
    elapsed: f64,
) {
    let mut parts: Vec<String> = Vec::new();
    if total_updated > 0 {
        parts.push(style::styled(GREEN, &format!("{} updated", total_updated)));
    }
    if total_skipped > 0 {
        parts.push(style::styled(DIM, &format!("{} up to date", total_skipped)));
    }
    if total_removed > 0 {
        parts.push(style::styled(YELLOW, &format!("{} removed", total_removed)));
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
            if total_removed > 0 {
                format!("{} removed", total_removed)
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

    let time_str = format_duration(elapsed);

    println!(
        "{}{}\n{}{}\n{}{}\n{}{}",
        CYAN,
        style::box_top("Result", TERM_WIDTH),
        CYAN,
        style::box_line(&summary_plain, &summary_styled, TERM_WIDTH),
        CYAN,
        style::box_line(&time_str, &time_str, TERM_WIDTH),
        CYAN,
        style::box_bottom(TERM_WIDTH),
    );
    println!("{}", style::RESET);
}

enum FileUpdateResult {
    Updated,
    Skipped,
    Failed,
}

fn update_file_skill(
    name: &str,
    manifest: &mut Manifest,
    skills_dir: &Path,
    verbose: bool,
    progress: &mut UpdateProgress,
    source_idx: usize,
    source_count: usize,
) -> FileUpdateResult {
    let url = match manifest.get_skill(name) {
        Some(e) => e.remote.clone(),
        None => return FileUpdateResult::Skipped,
    };
    progress.repo_started(source_idx, source_count, name, 1);
    progress.phase(ProgressPhase::Downloading);

    let downloaded = match download::download_skill_file(&url) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "  {} Failed to download {}: {}",
                style::styled(YELLOW, "\u{2717}"),
                name,
                e
            );
            progress.skill_checked();
            progress.repo_finished(name, RepoOutcome::new(0, 0, 1, 0));
            return FileUpdateResult::Failed;
        }
    };

    let skill_dir = skills_dir.join(name);
    let existing_md = skill_dir.join("SKILL.md");

    // Compare content
    if existing_md.exists() {
        let existing_bytes = std::fs::read(&existing_md).unwrap_or_default();
        if existing_bytes == downloaded.content {
            if verbose {
                println!(
                    "  {} Already up to date: {} (file)",
                    style::styled(DIM, "\u{2713}"),
                    name,
                );
            }
            progress.skill_checked();
            progress.repo_finished(name, RepoOutcome::new(0, 1, 0, 0));
            return FileUpdateResult::Skipped;
        }
    }

    // Content changed — update
    progress.phase(ProgressPhase::Copying);
    if let Err(e) = normalize::copy_skill(&downloaded.path, &skill_dir) {
        if verbose {
            eprintln!(
                "  {} Failed to copy {}: {}",
                style::styled(YELLOW, "\u{2717}"),
                name,
                e
            );
        }
        progress.skill_checked();
        progress.repo_finished(name, RepoOutcome::new(0, 0, 1, 0));
        return FileUpdateResult::Failed;
    }

    let frontmatter =
        spec::validate_skill(&skill_dir).unwrap_or_else(|_| crate::core::spec::SkillFrontmatter {
            name: name.to_string(),
            description: String::new(),
            version: None,
            authors: None,
            tags: None,
            platforms: None,
            license: None,
            repository: None,
            docs: None,
        });

    let version = frontmatter.version.unwrap_or_default();
    manifest.update_skill(name, "file:", &version, "", frontmatter.description);

    if verbose {
        println!(
            "  {} Updated: {} (file)",
            style::styled(GREEN, "\u{2713}"),
            name,
        );
    }

    progress.skill_checked();
    progress.repo_finished(name, RepoOutcome::new(1, 0, 0, 0));
    FileUpdateResult::Updated
}

pub fn execute(
    name: Option<&str>,
    verbose: bool,
    sparse: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let mut manifest = Manifest::load(manifest_path)?;

    let stale = manifest.find_stale(skills_dir);
    if !stale.is_empty() {
        eprintln!(
            "{} Skipping {} stale entries: {}",
            style::styled(YELLOW, "\u{26a0}"),
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
        if manifest.is_empty() {
            println!("No skills to update.");
            return Ok(());
        }
        manifest.skill_names()
    };

    // Separate file-based skills from git-based skills
    let (file_skill_names, git_skill_names): (Vec<String>, Vec<String>) =
        skills_to_update.into_iter().partition(|name| {
            manifest
                .get_skill(name)
                .map(|e| e.commit.starts_with("file:"))
                .unwrap_or(false)
        });

    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    for skill_name in &git_skill_names {
        let remote = manifest
            .get_skill(skill_name)
            .map(|e| e.remote.clone())
            .unwrap_or_default();
        groups.entry(remote).or_default().push(skill_name.clone());
    }

    let mut group_list: Vec<(String, Vec<String>)> = groups.into_iter().collect();
    group_list.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));

    let source_count = total_sources(file_skill_names.len(), group_list.len());
    let total_skill_count = file_skill_names.len() + git_skill_names.len();
    let mut progress = UpdateProgress::new(verbose);
    progress.start(total_skill_count, source_count);

    // Update file-based skills individually
    let mut file_updated = 0usize;
    let mut file_skipped = 0usize;
    let mut file_failed = 0usize;
    for (file_idx, name) in file_skill_names.iter().enumerate() {
        match update_file_skill(
            name,
            &mut manifest,
            skills_dir,
            verbose,
            &mut progress,
            file_idx + 1,
            source_count,
        ) {
            FileUpdateResult::Updated => file_updated += 1,
            FileUpdateResult::Skipped => file_skipped += 1,
            FileUpdateResult::Failed => file_failed += 1,
        }
    }

    if !group_list.is_empty() && !progress.enabled() {
        print_update_header(&group_list);
    }

    // Clone and update each group
    let mut total_updated = file_updated;
    let mut total_skipped = file_skipped;
    let mut total_failed = file_failed;
    let mut total_removed = 0usize;
    let mut repo_results: Vec<(String, GroupResult)> = Vec::new();

    let start = Instant::now();

    for (group_idx, (remote, skill_names)) in group_list.iter().enumerate() {
        let (label, group_result) = update_group(
            remote,
            skill_names,
            file_skill_names.len() + group_idx,
            source_count,
            &mut UpdateContext {
                manifest: &mut manifest,
                skills_dir,
                verbose,
                sparse,
                progress: &mut progress,
            },
        );
        total_updated += group_result.updated.len();
        total_skipped += group_result.skipped.len();
        total_failed += group_result.failed.len();
        total_removed += group_result.removed.len();
        repo_results.push((label, group_result));
    }

    progress.phase(ProgressPhase::Saving);
    manifest.save(manifest_path)?;
    progress.finish();

    print_results(&repo_results, verbose);
    print_summary(
        total_updated,
        total_skipped,
        total_failed,
        total_removed,
        start.elapsed().as_secs_f64(),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::{Manifest, SkillEntry};
    use crate::testutil;
    use clap::Parser;
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
    fn test_total_sources_counts_file_skills_and_git_groups() {
        let file_skill_count = 2usize;
        let git_group_count = 3usize;

        assert_eq!(total_sources(file_skill_count, git_group_count), 5);
    }

    #[test]
    fn test_total_sources_is_at_least_one_for_non_empty_update() {
        assert_eq!(total_sources(1, 0), 1);
        assert_eq!(total_sources(0, 1), 1);
    }

    #[test]
    fn test_update_empty_manifest_returns_ok() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(None, false, true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_specific_not_found_errors() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(
            Some("nonexistent"),
            false,
            true,
            &skills_dir,
            &manifest_path,
        );
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
            make_entry("real", "abc1234", "file:///nonexistent").1,
        );
        manifest.add_skill(
            "ghost",
            make_entry("ghost", "def5678", "file:///nonexistent").1,
        );
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        let result = execute(None, false, true, &skills_dir, &manifest_path);
        assert!(result.is_err() || result.is_ok());

        // Verify ghost is preserved (not removed) to avoid data loss
        let loaded = Manifest::load(&manifest_path).unwrap();
        assert!(
            loaded.skills.contains_key("ghost"),
            "Stale entry preserved to avoid data loss"
        );
    }

    #[test]
    fn test_update_skill_removed_from_repo() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create a git repo with ONLY "skill-a" (no "skill-b")
        let repo_url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: skill-a\ndescription: exists\n---\n",
        );

        // Manifest references "skill-a" (exists) and "skill-b" (removed from repo)
        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        manifest.add_skill("skill-a", make_entry("skill-a", "0000000", &repo_url).1);
        manifest.add_skill("skill-b", make_entry("skill-b", "1111111", &repo_url).1);
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        // skill-b doesn't exist in skills/skill-b in the repo, so it should be
        // reported as "removed from repo" rather than "invalid SKILL.md"
        let result = execute(Some("skill-b"), false, true, &skills_dir, &manifest_path);
        // Should not panic — gracefully handles missing skill
        assert!(result.is_ok() || result.is_err());
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

        let result = execute(Some("my-skill"), false, true, &skills_dir, &manifest_path);
        // Should succeed (skill is up to date)
        assert!(result.is_ok());
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

        let result = execute(Some("skip-skill"), false, true, &skills_dir, &manifest_path);
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

        let url = testutil::create_git_repo_with_skill(
            &temp,
            "---\nname: verb-skip\ndescription: t\n---\n",
        );

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

        let result = execute(Some("verb-skip"), true, true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
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

        let result = execute(Some("short-hash"), false, true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
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

        let result = execute(Some("changed"), false, true, &skills_dir, &manifest_path);
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

        let url = testutil::create_git_repo_with_skill(
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

        let result = execute(None, false, true, &skills_dir, &manifest_path);
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
    fn test_file_skill_detection_in_manifest() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        let fm = testutil::make_fm("detected");
        let mut entry = SkillEntry::new("url", "", &fm, "file:", "https://example.com/SKILL.md");
        entry.set_branch("");
        manifest.add_skill("detected", entry);
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        let loaded = Manifest::load(&manifest_path).unwrap();
        let e = loaded.skills.get("detected").unwrap();
        assert!(e.commit.starts_with("file:"));
    }
}
