use anyhow::Result;
use std::path::Path;

/// Remove an explicit target, preserving the service-facing command API.
pub fn execute(name: &str, force: bool, skills_dir: &Path, manifest_path: &Path) -> Result<()> {
    execute_options(Some(name), force, skills_dir, manifest_path)
}

pub fn execute_options(
    name: Option<&str>,
    force: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    use crate::tui::{self, ListMode, ListOutcome};
    let service = crate::services::SkillService::with_manifest_path(
        skills_dir.to_path_buf(),
        manifest_path.to_path_buf(),
    );
    let names = if let Some(name) = name {
        let names = service.resolve_removal_names(name)?;
        if !force {
            if tui::automatic_available() {
                let manifest = crate::core::manifest::Manifest::load(manifest_path)?;
                let mut rows: Vec<_> = super::list::manifest_rows(&manifest)
                    .into_iter()
                    .filter(|r| names.contains(&r.name))
                    .collect();
                if tui::run_list("Review skills to remove", &mut rows, ListMode::Review)?
                    != ListOutcome::Accepted
                {
                    println!("Aborted.");
                    return Ok(());
                }
            } else {
                use std::io::{self, Write};
                println!("Remove {} skill(s)?", names.len());
                for name in &names {
                    println!("  {}", name);
                }
                print!("Confirm removal [y/N]: ");
                io::stdout().flush()?;
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Aborted.");
                    return Ok(());
                }
            }
        }
        names
    } else {
        if force {
            anyhow::bail!("--force requires an explicit removal target");
        }
        tui::require_interactive()?;
        let manifest = crate::core::manifest::Manifest::load(manifest_path)?;
        let mut rows = super::list::manifest_rows(&manifest);
        if rows.is_empty() {
            println!("No tracked skills to remove.");
            return Ok(());
        }
        loop {
            if tui::run_list("Select skills to remove", &mut rows, ListMode::Select)?
                != ListOutcome::Accepted
            {
                println!("Aborted.");
                return Ok(());
            }
            let mut review: Vec<_> = rows.iter().filter(|r| r.selected).cloned().collect();
            match tui::run_list("Review skills to remove", &mut review, ListMode::Review)? {
                ListOutcome::Accepted => break review.into_iter().map(|r| r.name).collect(),
                ListOutcome::Back => continue,
                ListOutcome::Cancelled => {
                    println!("Aborted.");
                    return Ok(());
                }
            }
        }
    };
    let removed_names = service.remove_selected(&names)?;
    let mut report = crate::output::removal_report(&removed_names);
    if !crate::output::browse_report("Removal results", &mut report)? {
        for skill_name in removed_names {
            println!("Removed: {}", skill_name);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::{Manifest, SkillEntry};
    use crate::testutil;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_remove_untracked_errors() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute("ghost", true, &skills_dir, &manifest_path);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not installed by tome")
        );
    }

    #[test]
    fn test_remove_tracked_with_force() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("ToRemove")).unwrap();
        fs::write(
            skills_dir.join("ToRemove").join("SKILL.md"),
            "---\nname: ToRemove\ndescription: d\n---\n",
        )
        .unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "ToRemove",
            SkillEntry::new(
                "test/repo",
                "",
                &testutil::make_fm("ToRemove"),
                "abc",
                "url",
            ),
        );
        manifest.save(&manifest_path).unwrap();

        let result = execute("ToRemove", true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
        assert!(!skills_dir.join("ToRemove").exists());
    }

    #[test]
    fn test_remove_nonexistent_skill_dir_ok() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "NoDir",
            SkillEntry::new("test/repo", "", &testutil::make_fm("NoDir"), "abc", "url"),
        );
        manifest.save(&manifest_path).unwrap();

        // Remove tracked skill whose directory doesn't exist — should still succeed
        let result = execute("NoDir", true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }
}
