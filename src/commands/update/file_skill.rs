use std::path::Path;

use crate::core::{download, manifest::Manifest, spec};
use crate::fs;
use crate::progress::{ProgressPhase, RepoOutcome, UpdateProgress};
use crate::style::{self, DIM, GREEN, YELLOW};

pub(crate) enum FileUpdateResult {
    Updated,
    Skipped,
    Failed,
}

pub(crate) fn update_file_skill(
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

    progress.phase(ProgressPhase::Copying);
    if let Err(e) = fs::copy_skill(&downloaded.path, &skill_dir) {
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
