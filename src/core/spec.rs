use anyhow::{Context, Result, bail};
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

fn parse_list_value(value: &str) -> Vec<String> {
    let value = value.trim();
    if value.is_empty() {
        return vec![];
    }
    // Inline YAML list: [a, b, c]
    if value.starts_with('[') && value.ends_with(']') {
        return value[1..value.len() - 1]
            .split(',')
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    // Single item (not a list in frontmatter, but treat as single-element)
    vec![value.trim_matches('"').trim_matches('\'').to_string()]
}

fn parse_frontmatter_fallback(yaml: &str) -> Result<SkillFrontmatter> {
    let mut fm = SkillFrontmatter {
        name: String::new(),
        description: String::new(),
        version: None,
        authors: None,
        tags: None,
        platforms: None,
        license: None,
        repository: None,
        docs: None,
    };

    let mut collecting_authors = false;
    let mut authors_buf: Vec<String> = vec![];

    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Handle multi-line YAML list items (leading dash)
        if collecting_authors {
            if let Some(item) = trimmed.strip_prefix("- ") {
                authors_buf.push(item.trim_matches('"').trim_matches('\'').to_string());
                continue;
            } else {
                // End of multi-line list, save authors
                fm.authors = Some(authors_buf);
                authors_buf = vec![];
                collecting_authors = false;
                // Fall through to process this line as a new key
            }
        }

        // Split on the FIRST colon
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "name" => fm.name = unquote(value),
            "description" => fm.description = unquote(value),
            "version" => {
                if !value.is_empty() {
                    fm.version = Some(unquote(value));
                }
            }
            "license" => {
                if !value.is_empty() {
                    fm.license = Some(unquote(value));
                }
            }
            "repository" => {
                if !value.is_empty() {
                    fm.repository = Some(unquote(value));
                }
            }
            "docs" => {
                if !value.is_empty() {
                    fm.docs = Some(unquote(value));
                }
            }
            "authors" => {
                if value.is_empty() || value == "-" {
                    // Multi-line YAML list starting
                    collecting_authors = true;
                    authors_buf = vec![];
                } else {
                    fm.authors = Some(parse_list_value(value));
                }
            }
            "tags" if !value.is_empty() => {
                fm.tags = Some(parse_list_value(value));
            }
            "platforms" if !value.is_empty() => {
                fm.platforms = Some(parse_list_value(value));
            }
            _ => {}
        }
    }

    // Flush pending multi-line authors
    if collecting_authors && !authors_buf.is_empty() {
        fm.authors = Some(authors_buf);
    }

    if fm.name.is_empty() {
        bail!("Failed to parse frontmatter: name is required");
    }
    if fm.description.is_empty() {
        bail!("Failed to parse frontmatter: description is required");
    }

    Ok(fm)
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

pub fn parse_frontmatter(content: &str) -> Result<SkillFrontmatter> {
    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        bail!("No YAML frontmatter found (expected --- delimiters)");
    }
    let yaml_text = parts[1];

    // Try strict YAML parser first, then fallback for edge-case frontmatter
    // (long unquoted descriptions with ": " inside that trip up strict YAML parsers)
    serde_yaml::from_str::<SkillFrontmatter>(yaml_text).or_else(|e| {
        parse_frontmatter_fallback(yaml_text)
            .map_err(|_| anyhow::anyhow!("Failed to parse YAML frontmatter: {}", e))
    })
}

pub fn validate_skill(dir_path: &Path) -> Result<SkillFrontmatter> {
    let skill_md = dir_path.join("SKILL.md");
    if !skill_md.exists() {
        bail!("SKILL.md not found at {}", skill_md.display());
    }
    let content = fs::read_to_string(&skill_md).context("Failed to read SKILL.md")?;
    // Note: name/description emptiness is already enforced by parse_frontmatter
    // (serde_yaml requires them, and the fallback parser bails on empty values).
    // Only the length check is unique to validation.
    let frontmatter = parse_frontmatter(&content)?;

    if frontmatter.name.len() > 50 {
        bail!(
            "Skill name must be under 50 characters (got {})",
            frontmatter.name.len()
        );
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
    fn test_parse_fallback_long_description_with_colon() {
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
    fn test_parse_fallback_inline_list() {
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
        assert_eq!(
            fm.tags,
            Some(vec!["rust".into(), "cli".into(), "yaml".into()])
        );
        assert_eq!(fm.authors, Some(vec!["Alice".into(), "Bob".into()]));
        assert_eq!(fm.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn test_parse_fallback_multi_line_authors() {
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
        assert_eq!(fm.authors, Some(vec!["Alice".into(), "Bob".into()]));
    }

    #[test]
    fn test_parse_fallback_real_world_diagnose() {
        // This is the actual YAML that fails with strict serde_yaml
        let yaml = r#"name: diagnose
description: Diagnose a product against Ivan Zamesin's Next Move Theory / Advanced Jobs To Be Done methodology (distinct from generic Christensen JTBD) and route to the next move. A conversational, chat-first skill — through up to ~15 adaptive questions it challenges the goal you walked in with (climbing your business-Job graph to find a higher-leverage growth point), then surfaces EVERYTHING the methodology can find: all the risks (every weak node on the chain to profit, each traced to its upstream cause), all the growth points (kill a Job, Previous/Next Job, climb a level, adjacent-segment Small Jobs, underserved success criteria), and the risky assumptions hiding inside your current initiatives (RAT). It is comprehensive on findings and focused on action — it lists them all, then says which one to tackle first and which existing skill executes it (market-research, craft-value-proposition, product-requirements, craft-go-to-market, ask-nmt). The front door for LIVE products the way market-research is the front door for new ideas. Use when the user says "diagnose my product", "what should I do next", "a metric dropped", "where are my risks / growth points", "where is this breaking", or is unsure what to work on. Chat-first: writes nothing unless asked; recommends the next skill but the user launches it. Plain language; defaults to English.
user-invocable: true
"#;
        let fm = parse_frontmatter_fallback(yaml).unwrap();
        assert_eq!(fm.name, "diagnose");
        assert!(
            fm.description
                .contains("methodology can find: all the risks"),
            "Fallback should handle colons inside description, got: {}",
            &fm.description[..fm.description.len().min(200)]
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
    fn test_parse_fallback_missing_required() {
        let result = parse_frontmatter_fallback("version: 1.0.0");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("name"));
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
