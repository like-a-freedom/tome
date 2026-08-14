use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::fs;
use std::path::Path;

pub const MAX_SKILL_NAME_LEN: usize = 50;

#[derive(Deserialize, Debug, Clone)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
}

fn preprocess_frontmatter(yaml: &str) -> String {
    yaml.lines()
        .map(preprocess_frontmatter_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn preprocess_frontmatter_line(line: &str) -> String {
    let trimmed = line.trim_start();
    let Some(description) = trimmed.strip_prefix("description:") else {
        return line.to_string();
    };

    let value = description.trim();
    if value.is_empty()
        || !value.contains(": ")
        || value.starts_with('"')
        || value.starts_with('\'')
        || value.starts_with('|')
        || value.starts_with('>')
    {
        return line.to_string();
    }

    let indentation_len = line.len() - trimmed.len();
    let prefix_len = indentation_len + "description:".len();
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("{} \"{}\"", &line[..prefix_len], escaped)
}

pub fn parse_frontmatter(content: &str) -> Result<SkillFrontmatter> {
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        bail!("No YAML frontmatter found (expected --- delimiters)");
    }
    let yaml_text = parts[1];

    let preprocessed_yaml = preprocess_frontmatter(yaml_text);
    serde_yaml::from_str::<SkillFrontmatter>(&preprocessed_yaml)
        .map_err(|error| anyhow::anyhow!("Failed to parse YAML frontmatter: {error}"))
}

pub(crate) fn validate_skill_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Skill name must not be empty");
    }
    if name.len() > MAX_SKILL_NAME_LEN {
        bail!(
            "Skill name must be under {} characters (got {})",
            MAX_SKILL_NAME_LEN,
            name.len()
        );
    }
    if name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains(':')
        || name.chars().any(|character| character.is_control())
    {
        bail!("Skill name must be a safe single directory name");
    }
    Ok(())
}

pub fn validate_skill(dir_path: &Path) -> Result<SkillFrontmatter> {
    let skill_md = dir_path.join("SKILL.md");
    if !skill_md.exists() {
        bail!("SKILL.md not found at {}", skill_md.display());
    }
    let content = fs::read_to_string(&skill_md).context("Failed to read SKILL.md")?;

    let frontmatter = parse_frontmatter(&content)?;

    validate_skill_name(&frontmatter.name)?;

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
    }

    #[test]
    fn test_parse_minimal_frontmatter() {
        let content = "---\nname: Minimal\ndescription: Just enough\n---\n\nBody text.";
        let fm = parse_frontmatter(content).unwrap();
        assert_eq!(fm.name, "Minimal");
        assert_eq!(fm.description, "Just enough");
        assert!(fm.version.is_none());
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
        fs::write(
            &skill_md,
            "---\nname: My Skill\ndescription: A test\n---\n\n# My Skill\n",
        )
        .unwrap();

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
    fn test_parse_description_with_colons() {
        let content = r#"---
name: diagnose
description: Diagnose a product against a methodology: it finds all the risks (every weak node: traced upstream), all the growth points, and says which to tackle first.
user-invocable: true
---
"#;
        let fm = parse_frontmatter(content).unwrap();
        assert_eq!(fm.name, "diagnose");
        assert!(
            fm.description
                .contains("methodology: it finds all the risks"),
            "Should preserve full description with colons: {:?}",
            fm.description
        );
        assert!(fm.description.contains("tackle first."));
    }

    #[test]
    fn test_parse_inline_lists() {
        let content = r#"---
name: test
description: desc
tags: [rust, cli, yaml]
authors: ["Alice", 'Bob']
version: 1.0.0
---
"#;
        let fm = parse_frontmatter(content).unwrap();
        assert_eq!(fm.name, "test");
        assert_eq!(fm.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn test_parse_ignores_unknown_metadata_fields() {
        let content = r#"---
name: test
description: desc
authors:
  - Alice
  - Bob
---
"#;
        let fm = parse_frontmatter(content).unwrap();
        assert_eq!(fm.name, "test");
        assert_eq!(fm.description, "desc");
    }

    #[test]
    fn test_parse_real_world_description_with_colons() {
        let yaml = r#"name: diagnose
description: Diagnose a product against Ivan Zamesin's Next Move Theory / Advanced Jobs To Be Done methodology (distinct from generic Christensen JTBD) and route to the next move. A conversational, chat-first skill — through up to ~15 adaptive questions it challenges the goal you walked in with (climbing your business-Job graph to find a higher-leverage growth point), then surfaces EVERYTHING the methodology can find: all the risks (every weak node on the chain to profit, each traced to its upstream cause), all the growth points (kill a Job, Previous/Next Job, climb a level, adjacent-segment Small Jobs, underserved success criteria), and the risky assumptions hiding inside your current initiatives (RAT). It is comprehensive on findings and focused on action — it lists them all, then says which one to tackle first and which existing skill executes it (market-research, craft-value-proposition, product-requirements, craft-go-to-market, ask-nmt). The front door for LIVE products the way market-research is the front door for new ideas. Use when the user says "diagnose my product", "what should I do next", "a metric dropped", "where are my risks / growth points", "where is this breaking", or is unsure what to work on. Chat-first: writes nothing unless asked; recommends the next skill but the user launches it. Plain language; defaults to English.
user-invocable: true
"#;
        let content = format!("---\n{}---\n", yaml);
        let fm = parse_frontmatter(&content).unwrap();
        assert_eq!(fm.name, "diagnose");
        assert!(
            fm.description
                .contains("methodology can find: all the risks"),
            "Description should preserve colons, got: {}",
            fm.description
        );
        assert!(
            fm.description.starts_with("Diagnose a product against"),
            "Description should start correctly"
        );
        assert!(
            fm.description.contains("Chat-first: writes nothing"),
            "Second colon in description should be preserved"
        );
    }

    #[test]
    fn test_parse_missing_required() {
        let result = parse_frontmatter("---\nversion: 1.0.0\n---\n");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name"));
    }

    #[test]
    fn test_validate_skill_name_rejects_path_components() {
        for name in ["../escape", "nested/skill", "..", "\\\\escape"] {
            assert!(
                validate_skill_name(name).is_err(),
                "accepted unsafe name: {name}"
            );
        }
        assert!(validate_skill_name("safe skill").is_ok());
    }

    #[test]
    fn test_validate_skill_rejects_unsafe_frontmatter_name() {
        let temp = TempDir::new().unwrap();
        let skill_md = temp.path().join("SKILL.md");
        fs::write(
            &skill_md,
            "---\nname: ../escape\ndescription: unsafe\n---\n",
        )
        .unwrap();

        assert!(validate_skill(temp.path()).is_err());
    }

    #[test]
    fn test_validate_skill_name_too_long() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("bad-skill");
        fs::create_dir(&skill_dir).unwrap();

        let long_name = "a".repeat(51);
        let skill_md = skill_dir.join("SKILL.md");
        fs::write(
            &skill_md,
            format!("---\nname: {}\ndescription: Test\n---\n", long_name),
        )
        .unwrap();

        let result = validate_skill(&skill_dir);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("50"));
    }
}
