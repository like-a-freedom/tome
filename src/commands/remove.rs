use anyhow::Result;
use std::path::Path;

/// Thin adapter: CLI args → SkillService call → output formatting.
pub fn execute(name: &str, force: bool, skills_dir: &Path, manifest_path: &Path) -> Result<()> {
    let service = crate::services::SkillService::with_manifest_path(
        skills_dir.to_path_buf(),
        manifest_path.to_path_buf(),
    );
    let names = service.resolve_removal_names(name)?;

    if !force {
        let label = if names.len() == 1 {
            format!("Remove skill '{}'?", names[0])
        } else {
            format!(
                "Remove {} skills from '{}'? ({})",
                names.len(),
                name,
                names.join(", ")
            )
        };
        print!("{} [y/N]: ", label);
        use std::io::{self, Write};
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().eq_ignore_ascii_case("y") {
            // Continue with the service call below.
        } else {
            println!("Aborted.");
            return Ok(());
        }
    }

    let removed_names = service.remove(name)?;
    for skill_name in removed_names {
        println!("Removed: {}", skill_name);
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
