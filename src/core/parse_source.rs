use anyhow::Result;

const GITHUB_URL_PREFIX: &str = "https://github.com/";

fn is_url(source: &str) -> bool {
    source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("file://")
        || source.starts_with("git@")
        || source.starts_with("ssh://")
}

pub fn github_url(owner: &str, repo: &str) -> String {
    format!("{}{}/{}.git", GITHUB_URL_PREFIX, owner, repo)
}

pub fn repo_label(url: &str) -> &str {
    let without_prefix = url.strip_prefix(GITHUB_URL_PREFIX).unwrap_or(url);
    without_prefix
        .strip_suffix(".git")
        .unwrap_or(without_prefix)
}

/// Resolved source: either a raw URL or an owner/repo pair.
#[derive(Debug)]
pub enum Source {
    Url(String),
    OwnerRepo {
        owner: String,
        repo: String,
        skill: Option<String>,
    },
}

impl Source {
    pub fn parse(source: &str) -> Result<Self> {
        if is_url(source) {
            return Ok(Source::Url(source.to_string()));
        }
        let parsed = parse_source(source)?;
        Ok(Source::OwnerRepo {
            owner: parsed.owner,
            repo: parsed.repo,
            skill: parsed.skill,
        })
    }

    pub fn git_url(&self) -> String {
        match self {
            Source::Url(url) => url.clone(),
            Source::OwnerRepo { owner, repo, .. } => github_url(owner, repo),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Source::Url(url) => repo_label(url).to_string(),
            Source::OwnerRepo { owner, repo, .. } => format!("{}/{}", owner, repo),
        }
    }

    pub fn skill_name(&self) -> Option<&str> {
        match self {
            Source::Url(_) => None,
            Source::OwnerRepo { skill, .. } => skill.as_deref(),
        }
    }
}

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

    let (owner, repo) = repo_part.split_once('/').ok_or_else(|| {
        anyhow::anyhow!("Invalid source format. Expected: owner/repo or owner/repo@skill-name")
    })?;

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

    #[test]
    fn test_repo_label_url_with_git_suffix() {
        assert_eq!(
            repo_label("https://github.com/owner/repo.git"),
            "owner/repo"
        );
    }

    #[test]
    fn test_repo_label_url_without_git_suffix() {
        assert_eq!(repo_label("https://github.com/owner/repo"), "owner/repo");
    }

    #[test]
    fn test_repo_label_non_github_url() {
        assert_eq!(
            repo_label("https://gitlab.com/owner/repo"),
            "https://gitlab.com/owner/repo"
        );
    }

    #[test]
    fn test_repo_label_plain_string() {
        assert_eq!(repo_label("owner/repo"), "owner/repo");
    }
}
