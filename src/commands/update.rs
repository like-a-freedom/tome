use anyhow::Result;
use std::path::Path;

/// Thin adapter: CLI args → UpdateService call → output formatting.
pub fn execute(
    name: Option<&str>,
    verbose: bool,
    sparse: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let service = crate::services::UpdateService::with_manifest_path(
        skills_dir.to_path_buf(),
        manifest_path.to_path_buf(),
    );
    let mut progress = crate::progress::UpdateProgress::new(verbose);
    let start = std::time::Instant::now();
    let result = service.run(name, sparse, &mut progress)?;

    if !result.stale.is_empty() {
        crate::output::print_stale_header(result.stale.len(), &result.stale);
    }

    if result.is_empty() {
        progress.finish();
        println!("No skills to update.");
        return Ok(());
    }

    progress.finish();

    if verbose {
        for info in &result.updated_details {
            match info.source {
                crate::services::update_service::UpdateSource::File => {
                    crate::output::print_file_updated(&info.name);
                }
                crate::services::update_service::UpdateSource::Git => {
                    crate::output::print_updated(
                        &info.name,
                        &info.version,
                        &crate::output::format_commit(&info.old_commit),
                        &crate::output::format_commit(&info.new_commit),
                    );
                }
            }
        }
        for info in &result.skipped_details {
            let marker = match info.source {
                crate::services::update_service::UpdateSource::File => "file".to_string(),
                crate::services::update_service::UpdateSource::Git => {
                    crate::output::format_commit(&info.new_commit)
                }
            };
            crate::output::print_up_to_date(&info.name, &marker);
        }
        for name in &result.removed {
            crate::output::print_removed(name);
        }
    }
    for failure in &result.failures {
        crate::output::print_failed(&failure.name, "update", &failure.reason);
    }

    crate::output::print_update_summary(
        result.updated.len(),
        result.skipped.len(),
        result.failed.len(),
        result.removed.len(),
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
    fn test_update_stale_entries_preserved() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create real skill
        fs::create_dir_all(skills_dir.join("real")).unwrap();
        fs::write(
            skills_dir.join("real/SKILL.md"),
            "---\nname: real\ndescription: r\n---\n",
        )
        .unwrap();

        let mut manifest = Manifest::new();
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

        // Run update — stale entries should be preserved
        let _result = execute(None, false, true, &skills_dir, &manifest_path);

        // Verify manifest was saved
        let loaded = Manifest::load(&manifest_path).unwrap();
        assert!(loaded.is_tracked("ghost"), "Stale entry preserved");
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

        let mut manifest = Manifest::new();
        let fm = testutil::make_fm("detected");
        let mut entry = SkillEntry::new("url", "", &fm, "file:", "https://example.com/SKILL.md");
        entry.set_branch("");
        manifest.add_skill("detected", entry);
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();

        let loaded = Manifest::load(&manifest_path).unwrap();
        let e = loaded.get_skill("detected").unwrap();
        assert!(e.commit.starts_with("file:"));
    }
}
