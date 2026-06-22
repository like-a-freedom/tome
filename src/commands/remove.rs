use anyhow::{bail, Result};
use std::path::Path;

use crate::core::manifest::Manifest;
use crate::fs;

pub fn execute(
    name: &str,
    force: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    let mut manifest = Manifest::load(manifest_path)?;

    if !manifest.is_tracked(name) {
        bail!(
            "Skill '{}' is not installed by tome. Use `rm -rf` to remove manually-placed skills.",
            name
        );
    }

    if !force {
        print!("Remove skill '{}'? [y/N]: ", name);
        use std::io::{self, Write};
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim().to_lowercase() != "y" {
            println!("Aborted.");
            return Ok(());
        }
    }

    let skill_dir = skills_dir.join(name);
    fs::remove_skill_dir(&skill_dir)?;

    manifest.remove_skill(name);
    manifest.save(manifest_path)?;

    println!("Removed: {}", name);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::SkillEntry;
    use crate::core::spec::SkillFrontmatter;
    use std::fs;
    use tempfile::TempDir;

    fn make_fm(name: &str) -> SkillFrontmatter {
        SkillFrontmatter {
            name: name.to_string(),
            description: format!("{} desc", name),
            version: Some("1.0.0".into()),
            authors: None,
            tags: None,
            platforms: None,
            license: None,
            repository: None,
            docs: None,
        }
    }

    #[test]
    fn test_remove_untracked_errors() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute("ghost", true, &skills_dir, &manifest_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not installed by tome"));
    }

    #[test]
    fn test_remove_tracked_with_force() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("ToRemove")).unwrap();
        fs::write(skills_dir.join("ToRemove").join("SKILL.md"), "---\nname: ToRemove\ndescription: d\n---\n").unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        manifest.add_skill("ToRemove", SkillEntry::new("test/repo", "", &make_fm("ToRemove"), "abc", "url"));
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
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        manifest.add_skill("NoDir", SkillEntry::new("test/repo", "", &make_fm("NoDir"), "abc", "url"));
        manifest.save(&manifest_path).unwrap();

        // Remove tracked skill whose directory doesn't exist — should still succeed
        let result = execute("NoDir", true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }
}
