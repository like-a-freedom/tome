use anyhow::Result;

#[derive(Debug)]
pub struct ParsedSource {
    pub owner: String,
    pub repo: String,
    pub skill: Option<String>,
}

pub fn parse_source(source: &str) -> Result<ParsedSource> {
    let (repo_part, skill) = match source.split_once('@') {
        Some((r, s)) => (r, Some(s.to_string())),
        None => (source, None),
    };

    let (owner, repo) = repo_part
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!(
            "Invalid source format. Expected: owner/repo or owner/repo@skill-name"
        ))?;

    if owner.is_empty() || repo.is_empty() {
        anyhow::bail!("Invalid source format. Expected: owner/repo or owner/repo@skill-name");
    }

    Ok(ParsedSource {
        owner: owner.to_string(),
        repo: repo.to_string(),
        skill,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let p = parse_source("owner/repo").unwrap();
        assert_eq!(p.owner, "owner");
        assert_eq!(p.repo, "repo");
        assert!(p.skill.is_none());
    }

    #[test]
    fn test_parse_with_skill() {
        let p = parse_source("owner/repo@my-skill").unwrap();
        assert_eq!(p.owner, "owner");
        assert_eq!(p.repo, "repo");
        assert_eq!(p.skill.as_deref(), Some("my-skill"));
    }

    #[test]
    fn test_parse_with_dots_and_hyphens() {
        let p = parse_source("org-name/repo.name@skill-v2.1").unwrap();
        assert_eq!(p.owner, "org-name");
        assert_eq!(p.repo, "repo.name");
        assert_eq!(p.skill.as_deref(), Some("skill-v2.1"));
    }

    #[test]
    fn test_parse_no_slash() {
        let result = parse_source("justastring");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("owner/repo"));
    }

    #[test]
    fn test_parse_empty_parts() {
        let result = parse_source("/repo");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_empty_repo() {
        let result = parse_source("owner/");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_multiple_at_symbols() {
        let p = parse_source("owner/repo@skill@extra").unwrap();
        assert_eq!(p.skill.as_deref(), Some("skill@extra"));
    }
}
