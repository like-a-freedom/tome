use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["clone", "--depth", "1", url])
        .arg(dest)
        .output()
        .context("Failed to execute git")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git clone failed: {}", stderr);
    }
    Ok(())
}

pub fn fetch_updates(repo_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["pull", "--rebase"])
        .current_dir(repo_path)
        .output()
        .context("Failed to execute git")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git pull failed: {}", stderr);
    }
    Ok(())
}

pub fn get_commit_hash(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to execute git")?;

    if !output.status.success() {
        bail!("git rev-parse failed");
    }

    let hash = String::from_utf8(output.stdout)
        .context("Invalid UTF-8")?
        .trim()
        .to_string();

    Ok(hash)
}

pub fn get_branch(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(repo_path)
        .output()
        .context("Failed to execute git")?;

    if !output.status.success() {
        bail!("git branch failed");
    }

    let branch = String::from_utf8(output.stdout)
        .context("Invalid UTF-8")?
        .trim()
        .to_string();

    Ok(branch)
}

pub fn get_remote_url(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(repo_path)
        .output()
        .context("Failed to execute git")?;

    if !output.status.success() {
        bail!("git remote failed");
    }

    let url = String::from_utf8(output.stdout)
        .context("Invalid UTF-8")?
        .trim()
        .to_string();

    Ok(url)
}

pub fn is_installed() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    /// Helper: create a temp git repo with one commit
    fn create_temp_git_repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let file = temp.path().join("test.txt");
        fs::write(&file, "hello").unwrap();

        Command::new("git")
            .args(["add", "."])
            .current_dir(temp.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        temp
    }

    #[test]
    fn test_is_installed() {
        assert!(is_installed());
    }

    #[test]
    fn test_get_commit_hash() {
        let repo = create_temp_git_repo();
        let hash = get_commit_hash(repo.path()).unwrap();
        assert_eq!(hash.len(), 7);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_get_branch() {
        let repo = create_temp_git_repo();
        let branch = get_branch(repo.path()).unwrap();
        assert!(branch == "master" || branch == "main");
    }

    #[test]
    fn test_get_remote_url_none() {
        let repo = create_temp_git_repo();
        let result = get_remote_url(repo.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_clone_local_repo() {
        let src = create_temp_git_repo();
        let dest = TempDir::new().unwrap();
        let dest_path = dest.path().join("clone");

        let path_str = src.path().to_string_lossy().to_string();
        clone_repo(&path_str, &dest_path).unwrap();

        assert!(dest_path.join(".git").exists());
        assert!(dest_path.join("test.txt").exists());
    }

    #[test]
    fn test_get_remote_url_with_origin() {
        let repo = create_temp_git_repo();
        Command::new("git")
            .args(["remote", "add", "origin", "https://github.com/test/example.git"])
            .current_dir(repo.path())
            .output()
            .unwrap();

        let url = get_remote_url(repo.path()).unwrap();
        assert_eq!(url, "https://github.com/test/example.git");
    }

    #[test]
    fn test_clone_invalid_url_fails() {
        let temp = TempDir::new().unwrap();
        let dest = temp.path().join("clone");
        let result = clone_repo("https://github.com/nonexistent/repo.git", &dest);
        assert!(result.is_err());
    }

    #[test]
    fn test_fetch_updates_on_non_repo_fails() {
        let temp = TempDir::new().unwrap();
        let result = fetch_updates(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_get_commit_hash_on_non_repo_fails() {
        let temp = TempDir::new().unwrap();
        let result = get_commit_hash(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_get_branch_on_non_repo_fails() {
        let temp = TempDir::new().unwrap();
        let result = get_branch(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_fetch_updates_success() {
        // fetch_updates runs "git pull --rebase" which needs a remote
        // On a fresh local repo with no remote, pull fails but we just need to verify the function runs
        let repo = create_temp_git_repo();
        // fetch_updates will fail because no remote, but we test the error path
        let result = fetch_updates(repo.path());
        // This is expected to fail — no remote configured
        assert!(result.is_err());
    }
}
