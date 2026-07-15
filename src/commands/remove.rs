use anyhow::{Result, bail};
use std::path::Path;

use crate::core::manifest::Manifest;
use crate::fs;

pub fn execute(name: &str, force: bool, skills_dir: &Path, manifest_path: &Path) -> Result<()> {
    let mut manifest = Manifest::load(manifest_path)?;

    let names = manifest.resolve_names(name);
    if names.is_empty() {
        bail!(
            "Skill '{}' is not installed by tome. Use `rm -rf` to remove manually-placed skills.",
            name
        );
    }

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
        if input.trim().to_lowercase() != "y" {
            println!("Aborted.");
            return Ok(());
        }
    }

    for n in &names {
        let skill_dir = skills_dir.join(n);
        fs::remove_skill_dir(&skill_dir)?;
        manifest.remove_skill(n);
        println!("Removed: {}", n);
    }

    manifest.save(manifest_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::SkillEntry;
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
        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
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
        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
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
