use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

pub struct DownloadedSkill {
    pub path: PathBuf,
    pub content: Vec<u8>,
    pub source_url: String,
}

/// Check if curl is available on the system.
pub fn curl_available() -> bool {
    Command::new("curl")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Download a SKILL.md file from a URL using curl.
pub fn download_skill_file(url: &str) -> Result<DownloadedSkill> {
    if !curl_available() {
        anyhow::bail!("curl is required to download file URLs. Install curl and try again.");
    }

    let output = Command::new("curl")
        .args(["--silent", "--fail", "--location", url])
        .output()
        .with_context(|| format!("Failed to execute curl for {}", url))?;

    if output.status.code().is_none_or(|c| c != 0) {
        let code = output.status.code().unwrap_or(-1);
        anyhow::bail!("Failed to download {}: curl exited with code {}", url, code);
    }

    let content = output.stdout;
    if content.is_empty() {
        anyhow::bail!("Downloaded file is empty: {}", url);
    }

    // Validate it's valid UTF-8 (SKILL.md is text)
    std::str::from_utf8(&content)
        .with_context(|| format!("Downloaded file is not valid UTF-8: {}", url))?;

    let temp_dir =
        tempfile::tempdir().with_context(|| format!("Failed to create temp dir for {}", url))?;
    let skill_md = temp_dir.path().join("SKILL.md");
    std::fs::write(&skill_md, &content)
        .with_context(|| format!("Failed to write {}", skill_md.display()))?;

    Ok(DownloadedSkill {
        path: temp_dir.keep(),
        content,
        source_url: url.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_curl_available() {
        assert!(
            curl_available(),
            "curl must be installed to run download tests"
        );
    }

    #[test]
    fn test_download_skill_file_bad_url() {
        let result =
            download_skill_file("https://this-domain-does-not-exist-12345.example/SKILL.md");
        assert!(result.is_err());
    }
}
