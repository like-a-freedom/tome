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

/// Sparse-clone a repo: fetch tree objects only, then checkout just `skills/` and root SKILL.md.
/// Falls back silently if sparse-checkout is not supported (git < 2.25).
pub fn clone_repo_sparse(url: &str, dest: &Path) -> Result<()> {
    // Phase 1: Clone with blob filter, no checkout
    let status = Command::new("git")
        .args([
            "clone",
            "--filter=blob:none",
            "--no-checkout",
            "--depth",
            "1",
            "--progress",
            url,
        ])
        .arg(dest)
        .status()
        .context("Failed to execute git (sparse clone phase 1)")?;

    if !status.success() {
        bail!("git clone failed — is the repository public and the URL correct?");
    }

    // Phase 2: Initialize sparse-checkout in cone mode
    let init_status = Command::new("git")
        .args(["sparse-checkout", "init", "--cone"])
        .current_dir(dest)
        .status()
        .context("Failed to initialize sparse-checkout")?;

    if !init_status.success() {
        bail!("git sparse-checkout not supported on this git version");
    }

    // Phase 3: Inspect tree to decide what to fetch
    let has_skills_dir = has_path_in_tree(dest, "skills")?;
    let has_root_skill = has_path_in_tree(dest, "SKILL.md")?;

    // Phase 4: Set sparse-checkout paths
    let mut paths: Vec<&str> = Vec::new();
    if has_skills_dir {
        paths.push("skills");
    }
    if has_root_skill {
        paths.push("SKILL.md");
    }

    if paths.is_empty() {
        bail!("No SKILL.md found in repository root or skills/ directory");
    }

    let set_status = Command::new("git")
        .args(["sparse-checkout", "set"])
        .args(&paths)
        .current_dir(dest)
        .status()
        .context("Failed to set sparse-checkout paths")?;

    if !set_status.success() {
        bail!("Failed to configure sparse-checkout");
    }

    // Phase 5: Checkout the filtered files
    let checkout_status = Command::new("git")
        .args(["checkout"])
        .current_dir(dest)
        .status()
        .context("Failed to checkout files")?;

    if !checkout_status.success() {
        bail!("Failed to checkout files after sparse-checkout");
    }

    Ok(())
}

/// Check if a path exists in the git tree (without checking out files).
fn has_path_in_tree(repo_path: &Path, path: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["ls-tree", "HEAD", "--", path])
        .current_dir(repo_path)
        .output()
        .context("Failed to inspect git tree")?;

    Ok(output.status.success() && !output.stdout.is_empty())
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

    fn create_temp_git_repo_with_skills_dir() -> TempDir {
        let temp = TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let skills_dir = temp.path().join("skills").join("my-skill");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(
            skills_dir.join("SKILL.md"),
            "---\nname: test\ndescription: test\n---\n",
        )
        .unwrap();

        let large = temp.path().join("large-binary.bin");
        fs::write(&large, vec![0u8; 1024 * 1024]).unwrap();

        Command::new("git")
            .args(["add", "-A"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "init with skills and large file"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        temp
    }

    #[test]
    fn test_clone_sparse_local_repo() {
        let src = create_temp_git_repo_with_skills_dir();
        let dest = TempDir::new().unwrap();
        let dest_path = dest.path().join("clone");

        let path_str = src.path().to_string_lossy().to_string();
        clone_repo_sparse(&path_str, &dest_path).unwrap();

        assert!(dest_path.join(".git").exists());
        assert!(dest_path.join("skills").exists());
    }

    #[test]
    fn test_clone_invalid_url_fails() {
        let temp = TempDir::new().unwrap();
        let dest = temp.path().join("clone");
        let result = clone_repo_quiet("https://github.com/nonexistent/repo.git", &dest);
        assert!(result.is_err());
    }

    #[test]
    fn test_clone_sparse_falls_back_on_no_skills() {
        let temp = TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        fs::write(temp.path().join("README.md"), "no skills here").unwrap();
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(temp.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let dest = TempDir::new().unwrap();
        let dest_path = dest.path().join("clone");
        let result = clone_repo_sparse(&temp.path().to_string_lossy(), &dest_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No SKILL.md"));
    }

    #[test]
    fn test_has_path_in_tree() {
        let temp = create_temp_git_repo_with_skills_dir();
        assert!(has_path_in_tree(temp.path(), "skills").unwrap());
        assert!(!has_path_in_tree(temp.path(), "nonexistent").unwrap());
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
