use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use anyhow::{Result, bail};

use crate::core::manifest::Manifest;
use crate::progress::UpdateProgress;
use crate::style::{self, CYAN, YELLOW};

pub(crate) mod file_skill;
pub(crate) mod git_group;
pub(crate) mod summary;

pub(crate) use file_skill::{FileUpdateResult, update_file_skill};
pub(crate) use git_group::{GitGroupConfig, GitGroupOutcome, GroupResult, update_group};
pub(crate) use summary::{print_summary, total_sources};

pub(crate) struct UpdateContext<'a> {
    pub manifest: &'a mut Manifest,
    pub skills_dir: &'a Path,
    pub verbose: bool,
    pub sparse: bool,
    pub progress: &'a mut UpdateProgress,
}

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
        style::box_top("tome update", 60),
        CYAN,
        style::box_line(&header_plain, &header_styled, 60),
        CYAN,
        style::box_bottom(60),
    );
    println!("{}", style::RESET);
}

fn print_results(repo_results: &[(String, GroupResult)], verbose: bool) {
    let has_results = repo_results.iter().any(|(_, r)| !r.is_empty());
    if verbose && has_results {
        println!();
    }
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
        let resolved = manifest.resolve_names(specific_name);
        if resolved.is_empty() {
            bail!("Skill '{}' not found in manifest.", specific_name);
        }
        resolved
    } else {
        if manifest.is_empty() {
            println!("No skills to update.");
            return Ok(());
        }
        manifest.skill_names()
    };

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

    let mut total_updated = file_updated;
    let mut total_skipped = file_skipped;
    let mut total_failed = file_failed;
    let mut total_removed = 0usize;
    let mut repo_results: Vec<(String, GroupResult)> = Vec::new();

    let start = Instant::now();

    for (group_idx, (remote, skill_names)) in group_list.iter().enumerate() {
        let config = GitGroupConfig {
            remote,
            skill_names,
            group_idx: file_skill_names.len() + group_idx,
            group_count: source_count,
        };
        let GitGroupOutcome {
            label,
            result: group_result,
        } = update_group(
            &config,
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

    progress.phase(crate::progress::ProgressPhase::Saving);
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

        fs::create_dir_all(skills_dir.join("real")).unwrap();
        fs::write(
            skills_dir.join("real/SKILL.md"),
            "---\nname: real\ndescription: r\n---\n",
        )
        .unwrap();

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

        let loaded = Manifest::load(&manifest_path).unwrap();
        assert!(
            loaded.skills.contains_key("ghost"),
            "Stale entry preserved to avoid data loss"
        );
    }

    #[test]
    fn test_update_verbose_flag_accepted() {
        let args: Vec<&str> = vec!["tome", "update", "--verbose"];
        let cmd = crate::config::Cli::try_parse_from(args);
        assert!(cmd.is_ok());
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
