use anyhow::{Context, Result};
use std::path::Path;

pub fn create_temp_dir() -> Result<tempfile::TempDir> {
    tempfile::tempdir().context("Failed to create temp directory")
}

pub fn ensure_skills_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)
            .context("Failed to create skills directory")?;
    }
    Ok(())
}

pub fn remove_skill_dir(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path)
            .context("Failed to remove skill directory")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_temp_dir() {
        let temp = create_temp_dir().unwrap();
        assert!(temp.path().exists());
        let file = temp.path().join("test.txt");
        std::fs::write(&file, "data").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "data");
    }

    #[test]
    fn test_ensure_skills_dir_creates() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("agents").join("skills");
        ensure_skills_dir(&skills_dir).unwrap();
        assert!(skills_dir.exists());
    }

    #[test]
    fn test_ensure_skills_dir_already_exists() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("agents").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let result = ensure_skills_dir(&skills_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_remove_skill_dir() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("some-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "---\nname: x\ndescription: x\n---\n").unwrap();

        assert!(skill_dir.exists());
        remove_skill_dir(&skill_dir).unwrap();
        assert!(!skill_dir.exists());
    }

    #[test]
    fn test_remove_skill_dir_not_exists() {
        let temp = TempDir::new().unwrap();
        let result = remove_skill_dir(&temp.path().join("nonexistent"));
        assert!(result.is_ok());
    }
}
