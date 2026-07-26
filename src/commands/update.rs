use anyhow::Result;
use std::path::Path;

/// Update command implementation.
mod summary {
    use crate::style::{self, CYAN, DIM, GREEN, YELLOW};

    const TERM_WIDTH: usize = 60;
    const SHORT_COMMIT_LEN: usize = 7;

    pub(crate) fn format_commit(commit: &str) -> String {
        if commit.len() >= SHORT_COMMIT_LEN {
            commit[..SHORT_COMMIT_LEN].to_string()
        } else {
            commit.to_string()
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

    fn format_summary_line(summary_plain: &str, summary_styled: &str) -> String {
        format!(
            "{}{}",
            CYAN,
            style::box_line(
                summary_plain,
                &format!("{}{}", summary_styled, CYAN),
                TERM_WIDTH,
            )
        )
    }

    pub(crate) fn print_summary(
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
            plain_parts.join(" \u{00b7} ")
        };
        let summary_styled = parts.join(" \u{00b7} ");

        let time_str = format_duration(elapsed);

        println!(
            "{}{}\n{}\n{}{}\n{}{}",
            CYAN,
            style::box_top("Result", TERM_WIDTH),
            format_summary_line(&summary_plain, &summary_styled),
            CYAN,
            style::box_line(&time_str, &time_str, TERM_WIDTH),
            CYAN,
            style::box_bottom(TERM_WIDTH),
        );
        println!("{}", style::RESET);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_format_commit() {
            assert_eq!(format_commit("abc1234567890abcdef"), "abc1234");
            assert_eq!(format_commit("abc"), "abc");
        }

        #[test]
        fn test_summary_line_restores_frame_color_after_styled_segments() {
            let plain = "1 updated \u{00b7} 141 up to date";
            let styled = format!(
                "{} \u{00b7} {}",
                style::styled(GREEN, "1 updated"),
                style::styled(DIM, "141 up to date")
            );
            let rendered = format_summary_line(plain, &styled);
            let content_end = rendered.rfind(style::RESET).unwrap() + style::RESET.len();

            assert!(
                rendered[content_end..].starts_with(CYAN),
                "expected cyan frame after styled content, got {:?}",
                &rendered[content_end..]
            );
        }
    }
}

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
                        &summary::format_commit(&info.old_commit),
                        &summary::format_commit(&info.new_commit),
                    );
                }
            }
        }
        for info in &result.skipped_details {
            let marker = match info.source {
                crate::services::update_service::UpdateSource::File => "file".to_string(),
                crate::services::update_service::UpdateSource::Git => {
                    summary::format_commit(&info.new_commit)
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

    summary::print_summary(
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

        // Run update — stale entries should be preserved
        let _result = execute(None, false, true, &skills_dir, &manifest_path);

        // Verify manifest was saved
        let loaded = Manifest::load(&manifest_path).unwrap();
        assert!(loaded.skills.contains_key("ghost"), "Stale entry preserved");
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
