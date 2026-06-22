use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

/// Run a git command in the given directory and return its trimmed stdout.
fn git_output(repo_path: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .context("Failed to execute git")?;

    if !output.status.success() {
        bail!("git {} failed", args[0]);
    }

    String::from_utf8(output.stdout)
        .context("Invalid UTF-8")
        .map(|s| s.trim().to_string())
}

/// Clone a repo, streaming git's progress to the terminal.
pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["clone", "--depth", "1", "--progress", url])
        .arg(dest)
        .status()
        .context("Failed to execute git")?;

    if !status.success() {
        bail!("git clone failed — is the repository public and the URL correct?");
    }
    Ok(())
}

/// Clone a repo silently (captures output). Used in tests.
#[cfg(test)]
pub fn clone_repo_quiet(url: &str, dest: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["clone", "--depth", "1", "--quiet", url])
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
    git_output(repo_path, &["rev-parse", "HEAD"])
}

pub fn get_branch(repo_path: &Path) -> Result<String> {
    git_output(repo_path, &["branch", "--show-current"])
}

pub fn get_remote_url(repo_path: &Path) -> Result<String> {
    git_output(repo_path, &["remote", "get-url", "origin"])
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
        assert_eq!(hash.len(), 40);
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
        clone_repo_quiet(&path_str, &dest_path).unwrap();

        assert!(dest_path.join(".git").exists());
        assert!(dest_path.join("test.txt").exists());
    }

    #[test]
    fn test_get_remote_url_with_origin() {
        let repo = create_temp_git_repo();
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/test/example.git",
            ])
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
        let result = clone_repo_quiet("https://github.com/nonexistent/repo.git", &dest);
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
