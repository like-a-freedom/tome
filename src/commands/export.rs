use anyhow::{Context, Result};
use std::path::Path;

use crate::core::manifest::Manifest;

pub fn execute(_skills_dir: &Path, manifest_path: &Path) -> Result<()> {
    execute_options(false, _skills_dir, manifest_path)
}

pub fn execute_options(interactive: bool, _skills_dir: &Path, manifest_path: &Path) -> Result<()> {
    let selected = if interactive {
        crate::tui::require_interactive()?;
        let manifest = Manifest::load(manifest_path)?;
        let mut rows = super::list::manifest_rows(&manifest);
        if rows.is_empty() {
            return Ok(());
        }
        if crate::tui::run_list(
            "Select skills to export",
            &mut rows,
            crate::tui::ListMode::Select,
        )? != crate::tui::ListOutcome::Accepted
        {
            return Ok(());
        }
        Some(
            rows.into_iter()
                .filter(|r| r.selected)
                .map(|r| r.name)
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let output = execute_selected_to_string(manifest_path, selected.as_deref())?;
    if !output.is_empty() {
        print!("{}", output);
    }
    Ok(())
}

#[cfg(test)]
fn execute_to_string(manifest_path: &Path) -> Result<String> {
    execute_selected_to_string(manifest_path, None)
}

fn execute_selected_to_string(manifest_path: &Path, selected: Option<&[String]>) -> Result<String> {
    if !manifest_path.exists() {
        return Ok(String::new());
    }

    let manifest = Manifest::load(manifest_path).context("Failed to load manifest for export")?;

    if manifest.is_empty() {
        return Ok(String::new());
    }

    let mut entries: Vec<_> = manifest.all_skills();
    if let Some(selected) = selected {
        entries.retain(|(name, _)| selected.iter().any(|s| s == name));
    }
    entries.sort_by_key(|(name, _)| (*name).to_string());

    let mut lines: Vec<String> = Vec::new();
    for (_name, entry) in entries {
        let source = entry.install_source();
        if entry.path.is_empty() {
            lines.push(format!("tome add {}\n", source));
        } else {
            lines.push(format!("tome add {} --path {}\n", source, entry.path));
        }
    }

    Ok(lines.join(""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::{Manifest, SkillEntry};
    use crate::testutil;
    use std::fs;
    use tempfile::TempDir;

    fn make_temp_manifest(
        temp: &TempDir,
        manifest: &Manifest,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        manifest.save(&manifest_path).unwrap();
        (skills_dir, manifest_path)
    }

    #[test]
    fn test_export_empty_manifest() {
        let temp = TempDir::new().unwrap();
        let manifest = Manifest::new();
        let (_skills_dir, manifest_path) = make_temp_manifest(&temp, &manifest);
        let output = execute_to_string(&manifest_path).unwrap();
        assert_eq!(output, "");
    }

    #[test]
    fn test_export_single_git_skill() {
        let temp = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        let fm = testutil::make_fm("taste-skill");
        let entry = SkillEntry::new("Leonxlnx/taste-skill", "", &fm, "abc123", "");
        manifest.add_skill("taste-skill", entry);
        let (_skills_dir, manifest_path) = make_temp_manifest(&temp, &manifest);
        let output = execute_to_string(&manifest_path).unwrap();
        assert_eq!(output, "tome add Leonxlnx/taste-skill\n");
    }

    #[test]
    fn test_export_file_url_skill() {
        let temp = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        let fm = testutil::make_fm("file-skill");
        let entry = SkillEntry::new("", "", &fm, "abc123", "file:///Users/test/SKILL.md");
        manifest.add_skill("file-skill", entry);
        let (_skills_dir, manifest_path) = make_temp_manifest(&temp, &manifest);
        let output = execute_to_string(&manifest_path).unwrap();
        assert_eq!(output, "tome add file:///Users/test/SKILL.md\n");
    }

    #[test]
    fn test_export_nonexistent_manifest() {
        let temp = TempDir::new().unwrap();
        let manifest_path = temp.path().join("nonexistent").join(".skills.json");
        let output = execute_to_string(&manifest_path).unwrap();
        assert_eq!(output, "");
    }

    #[test]
    fn test_export_multiple_skills_sorted() {
        let temp = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        let fm1 = testutil::make_fm("zebra");
        let entry1 = SkillEntry::new("user/zebra", "", &fm1, "abc123", "");
        manifest.add_skill("zebra", entry1);
        let fm2 = testutil::make_fm("alpha");
        let entry2 = SkillEntry::new("user/alpha", "", &fm2, "def456", "");
        manifest.add_skill("alpha", entry2);
        let (_skills_dir, manifest_path) = make_temp_manifest(&temp, &manifest);
        let output = execute_to_string(&manifest_path).unwrap();
        assert_eq!(output, "tome add user/alpha\ntome add user/zebra\n");
    }

    #[test]
    fn test_export_with_path_override() {
        let temp = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        let fm = testutil::make_fm("nested-skill");
        let entry = SkillEntry::new("owner/repo", "skills/nested", &fm, "abc123", "");
        manifest.add_skill("nested-skill", entry);
        let (_skills_dir, manifest_path) = make_temp_manifest(&temp, &manifest);
        let output = execute_to_string(&manifest_path).unwrap();
        assert_eq!(output, "tome add owner/repo --path skills/nested\n");
    }

    #[test]
    fn test_export_via_execute_function() {
        let temp = TempDir::new().unwrap();
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "skill-b",
            SkillEntry::new(
                "owner/repo",
                "",
                &testutil::make_fm("skill-b"),
                "abc",
                "https://github.com/owner/repo.git",
            ),
        );
        manifest.add_skill(
            "skill-a",
            SkillEntry::new(
                "owner/other",
                "skills/a",
                &testutil::make_fm("skill-a"),
                "def",
                "https://github.com/owner/other.git",
            ),
        );
        let (skills_dir, manifest_path) = make_temp_manifest(&temp, &manifest);

        let result = execute(&skills_dir, &manifest_path);
        assert!(result.is_ok());
    }
    #[test]
    fn scalable_export_exact_selection_does_not_expand_repository() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("manifest.json");
        let mut manifest = Manifest::new();
        for name in ["alpha", "beta"] {
            manifest.add_skill(
                name,
                SkillEntry::new(
                    &format!("owner/repo@{name}"),
                    &format!("skills/{name}"),
                    &crate::testutil::make_fm(name),
                    "abc",
                    "https://github.com/owner/repo.git",
                ),
            );
        }
        manifest.save(&path).unwrap();
        assert_eq!(
            execute_selected_to_string(&path, Some(&["beta".into()])).unwrap(),
            "tome add owner/repo@beta --path skills/beta\n"
        );
        assert_eq!(execute_selected_to_string(&path, Some(&[])).unwrap(), "");
    }
}
