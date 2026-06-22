use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Deserialize, Debug)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub authors: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub platforms: Option<Vec<String>>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub docs: Option<String>,
}

pub fn parse_frontmatter(content: &str) -> Result<SkillFrontmatter> {
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        bail!("No YAML frontmatter found (expected --- delimiters)");
    }
    let frontmatter: SkillFrontmatter = serde_yaml::from_str(parts[1])
        .context("Failed to parse YAML frontmatter")?;
    Ok(frontmatter)
}

pub fn validate_skill(dir_path: &Path) -> Result<SkillFrontmatter> {
    let skill_md = dir_path.join("SKILL.md");
    if !skill_md.exists() {
        bail!("SKILL.md not found at {}", skill_md.display());
    }
    let content = fs::read_to_string(&skill_md)
        .context("Failed to read SKILL.md")?;
    let frontmatter = parse_frontmatter(&content)?;

    if frontmatter.name.is_empty() {
        bail!("Skill name is required");
    }
    if frontmatter.description.is_empty() {
        bail!("Skill description is required");
    }
    if frontmatter.name.len() > 50 {
        bail!("Skill name must be under 50 characters (got {})", frontmatter.name.len());
    }

    Ok(frontmatter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_parse_valid_frontmatter() {
        let content = r#"---
name: Test Skill
description: A test skill for validation
version: 1.0.0
authors:
  - test-author
tags: [testing, validation]
---

# Test Skill

This is the body.
"#;
        let fm = parse_frontmatter(content).unwrap();
        assert_eq!(fm.name, "Test Skill");
        assert_eq!(fm.description, "A test skill for validation");
        assert_eq!(fm.version.as_deref(), Some("1.0.0"));
        assert_eq!(fm.authors, Some(vec!["test-author".to_string()]));
        assert!(fm.tags.unwrap().contains(&"testing".to_string()));
    }

    #[test]
    fn test_parse_minimal_frontmatter() {
        let content = "---\nname: Minimal\ndescription: Just enough\n---\n\nBody text.";
        let fm = parse_frontmatter(content).unwrap();
        assert_eq!(fm.name, "Minimal");
        assert_eq!(fm.description, "Just enough");
        assert!(fm.version.is_none());
        assert!(fm.authors.is_none());
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "# Just markdown, no YAML";
        let result = parse_frontmatter(content);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("YAML frontmatter"));
    }

    #[test]
    fn test_parse_empty_frontmatter() {
        let content = "---\n---\nBody.";
        let result = parse_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_skill_creates_dir() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("my-skill");
        fs::create_dir(&skill_dir).unwrap();

        let skill_md = skill_dir.join("SKILL.md");
        fs::write(&skill_md, "---\nname: My Skill\ndescription: A test\n---\n\n# My Skill\n").unwrap();

        let fm = validate_skill(&skill_dir).unwrap();
        assert_eq!(fm.name, "My Skill");
    }

    #[test]
    fn test_validate_skill_missing_file() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("empty");
        fs::create_dir(&skill_dir).unwrap();

        let result = validate_skill(&skill_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("SKILL.md not found"));
    }

    #[test]
    fn test_validate_skill_missing_name() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("bad-skill");
        fs::create_dir(&skill_dir).unwrap();

        let skill_md = skill_dir.join("SKILL.md");
        fs::write(&skill_md, "---\ndescription: Forgot name\n---\n\nBody.").unwrap();

        let result = validate_skill(&skill_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_skill_name_too_long() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("bad-skill");
        fs::create_dir(&skill_dir).unwrap();

        let long_name = "a".repeat(51);
        let skill_md = skill_dir.join("SKILL.md");
        fs::write(&skill_md, format!("---\nname: {}\ndescription: Test\n---\n", long_name)).unwrap();

        let result = validate_skill(&skill_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("50"));
    }
}
