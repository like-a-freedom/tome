use anyhow::{Result, bail};

const GITHUB_URL_PREFIX: &str = "https://github.com/";

pub(crate) fn is_url(source: &str) -> bool {
    source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("file://")
        || source.starts_with("git@")
        || source.starts_with("ssh://")
}

fn is_file_url(source: &str) -> bool {
    if !is_url(source) {
        return false;
    }
    let path = source
        .find("://")
        .map(|i| &source[i + 3..])
        .unwrap_or(source);
    let lower = path.to_lowercase();
    lower.ends_with(".md") || lower.contains("skill.md")
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
    FileUrl {
        url: String,
    },
}

impl Source {
    pub fn parse(source: &str) -> Result<Self> {
        if is_file_url(source) {
            return Ok(Source::FileUrl {
                url: source.to_string(),
            });
        }

        if is_url(source) {
            return Ok(Source::Url(source.to_string()));
        }

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

        Ok(Source::OwnerRepo {
            owner: owner.to_string(),
            repo: repo.to_string(),
            skill,
        })
    }

    pub fn git_url(&self) -> Result<String> {
        match self {
            Source::Url(url) => Ok(url.clone()),
            Source::OwnerRepo { owner, repo, .. } => Ok(github_url(owner, repo)),
            Source::FileUrl { .. } => bail!("Cannot clone a file URL. Use the download flow."),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Source::Url(url) => repo_label(url).to_string(),
            Source::OwnerRepo { owner, repo, .. } => format!("{}/{}", owner, repo),
            Source::FileUrl { url } => {
                let path = url.find("://").map(|i| &url[i + 3..]).unwrap_or(url);
                let segment = path
                    .split('/')
                    .rev()
                    .find(|s| !s.is_empty() && *s != "SKILL.md" && *s != "skill.md")
                    .unwrap_or("skill");
                segment.trim_end_matches(".md").to_string()
            }
        }
    }

    pub fn skill_name(&self) -> Option<&str> {
        match self {
            Source::Url(_) | Source::FileUrl { .. } => None,
            Source::OwnerRepo { skill, .. } => skill.as_deref(),
        }
    }

    pub fn is_file_url(&self) -> bool {
        matches!(self, Source::FileUrl { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let s = Source::parse("owner/repo").unwrap();
        assert_eq!(s.label(), "owner/repo");
        assert!(s.skill_name().is_none());
    }

    #[test]
    fn test_parse_with_skill() {
        let s = Source::parse("owner/repo@my-skill").unwrap();
        assert_eq!(s.label(), "owner/repo");
        assert_eq!(s.skill_name(), Some("my-skill"));
    }

    #[test]
    fn test_parse_with_dots_and_hyphens() {
        let s = Source::parse("org-name/repo.name@skill-v2.1").unwrap();
        assert_eq!(s.label(), "org-name/repo.name");
        assert_eq!(s.skill_name(), Some("skill-v2.1"));
    }

    #[test]
    fn test_parse_no_slash() {
        let result = Source::parse("justastring");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("owner/repo"));
    }

    #[test]
    fn test_parse_empty_parts() {
        assert!(Source::parse("/repo").is_err());
    }

    #[test]
    fn test_parse_empty_repo() {
        assert!(Source::parse("owner/").is_err());
    }

    #[test]
    fn test_parse_multiple_at_symbols() {
        let s = Source::parse("owner/repo@skill@extra").unwrap();
        assert_eq!(s.skill_name(), Some("skill@extra"));
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

    #[test]
    fn test_parse_file_url() {
        let s = Source::parse("https://example.com/SKILL.md").unwrap();
        assert!(s.is_file_url());
        assert_eq!(s.label(), "example.com");
    }

    #[test]
    fn test_parse_file_url_with_path() {
        let s = Source::parse("https://example.com/skills/my-skill.md").unwrap();
        assert!(s.is_file_url());
        assert_eq!(s.label(), "my-skill");
    }

    #[test]
    fn test_parse_non_md_url_not_file_url() {
        let s = Source::parse("https://github.com/owner/repo").unwrap();
        assert!(!s.is_file_url());
    }

    #[test]
    fn test_file_url_git_url_errors() {
        let s = Source::parse("https://example.com/SKILL.md").unwrap();
        let result = s.git_url();
        assert!(result.is_err());
    }
}
