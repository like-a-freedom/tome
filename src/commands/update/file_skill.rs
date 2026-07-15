use std::path::Path;

use crate::core::{download, manifest::Manifest, spec};
use crate::fs;
use crate::output;
use crate::progress::{ProgressPhase, RepoOutcome, UpdateProgress};

pub(crate) enum FileUpdateResult {
    Updated,
    Skipped,
    Failed,
}

/// Check if a file-skill needs updating by comparing existing bytes with new bytes.
fn content_matches(existing: &[u8], new_content: &[u8]) -> bool {
    existing == new_content
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
            output::print_failed(name, "download", &e.to_string());
            progress.skill_checked();
            progress.repo_finished(name, RepoOutcome::new(0, 0, 1, 0));
            return FileUpdateResult::Failed;
        }
    };

    let skill_dir = skills_dir.join(name);
    let existing_md = skill_dir.join("SKILL.md");

    if existing_md.exists() {
        let existing_bytes = std::fs::read(&existing_md).unwrap_or_default();
        if content_matches(&existing_bytes, &downloaded.content) {
            if verbose {
                output::print_up_to_date(name, "file");
            }
            progress.skill_checked();
            progress.repo_finished(name, RepoOutcome::new(0, 1, 0, 0));
            return FileUpdateResult::Skipped;
        }
    }

    progress.phase(ProgressPhase::Copying);
    if let Err(e) = fs::copy_skill(&downloaded.path, &skill_dir) {
        if verbose {
            output::print_failed(name, "copy", &e.to_string());
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
        output::print_file_updated(name);
    }

    progress.skill_checked();
    progress.repo_finished(name, RepoOutcome::new(1, 0, 0, 0));
    FileUpdateResult::Updated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::{Manifest, SkillEntry};
    use crate::core::spec::SkillFrontmatter;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_manifest_with_file_skill(
        name: &str,
        remote: &str,
        content: &[u8],
    ) -> (TempDir, PathBuf, Manifest) {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let skill_dir = skills_dir.join(name);
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

        let mut manifest = Manifest {
            version: 1,
            skills: HashMap::new(),
        };
        let fm = SkillFrontmatter {
            name: name.to_string(),
            description: "test".to_string(),
            version: None,
            authors: None,
            tags: None,
            platforms: None,
            license: None,
            repository: None,
            docs: None,
        };
        let mut entry = SkillEntry::new("file-url", "", &fm, "file:", remote);
        entry.set_branch("");
        manifest.add_skill(name, entry);

        (temp, skills_dir, manifest)
    }

    fn make_progress() -> UpdateProgress {
        UpdateProgress::new(false)
    }

    #[test]
    fn test_content_matches_identical() {
        assert!(content_matches(b"hello", b"hello"));
    }

    #[test]
    fn test_content_matches_both_empty() {
        assert!(content_matches(b"", b""));
    }

    #[test]
    fn test_content_matches_different_lengths() {
        assert!(!content_matches(b"short", b"longer content"));
    }

    #[test]
    fn test_content_matches_same_length_different() {
        assert!(!content_matches(b"abcde", b"fghij"));
    }

    #[test]
    fn test_content_matches_trailing_newline_matters() {
        assert!(!content_matches(b"hello\n", b"hello"));
    }

    #[test]
    fn test_update_file_skill_not_in_manifest() {
        let (_temp, skills_dir, mut manifest) = make_manifest_with_file_skill(
            "my-skill",
            "file:///tmp/nonexistent.md",
            b"---\nname: my-skill\ndescription: d\n---\n",
        );
        let mut progress = make_progress();
        let result = update_file_skill(
            "ghost-skill",
            &mut manifest,
            &skills_dir,
            false,
            &mut progress,
            1,
            1,
        );
        assert!(matches!(result, FileUpdateResult::Skipped));
    }

    #[test]
    fn test_print_file_up_to_date_format() {
        // Ensures the file marker "file" stays as user-facing format
        let out = output::format_up_to_date("my-skill", "file");
        assert!(out.contains("my-skill"));
        assert!(out.contains("(file)"));
    }

    #[test]
    fn test_print_file_updated_format() {
        let out = output::format_file_updated("my-skill");
        assert!(out.contains("my-skill"));
        assert!(out.contains("(file)"));
    }

    #[test]
    fn test_file_update_result_variants() {
        // Regression: ensure all three variants exist and are distinguishable
        let updated = FileUpdateResult::Updated;
        let skipped = FileUpdateResult::Skipped;
        let failed = FileUpdateResult::Failed;
        assert!(matches!(updated, FileUpdateResult::Updated));
        assert!(matches!(skipped, FileUpdateResult::Skipped));
        assert!(matches!(failed, FileUpdateResult::Failed));
        assert!(!matches!(updated, FileUpdateResult::Skipped));
        assert!(!matches!(skipped, FileUpdateResult::Failed));
        assert!(!matches!(failed, FileUpdateResult::Updated));
    }

    #[test]
    fn test_make_manifest_with_file_skill_helper() {
        let (_temp, skills_dir, manifest) = make_manifest_with_file_skill(
            "helper-test",
            "https://example.com/SKILL.md",
            b"test content",
        );
        assert!(manifest.is_tracked("helper-test"));
        let entry = manifest.get_skill("helper-test").unwrap();
        assert!(entry.commit.starts_with("file:"));
        assert_eq!(entry.branch, "");
        assert!(skills_dir.join("helper-test").join("SKILL.md").exists());
    }

    #[test]
    fn test_print_failed_skills_download_includes_name() {
        let out = output::format_failed("my-skill", "download", "404 not found");
        assert!(out.contains("Failed to download my-skill: 404 not found"));
    }
}
