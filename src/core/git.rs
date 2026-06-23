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

/// Clone a repo, preferring sparse clone with fallback to full clone when:
/// - sparse-checkout is not supported (git < 2.25)
/// - repository has no `skills/` or root SKILL.md
pub fn clone_repo_or_sparse(url: &str, dest: &Path, sparse: bool) -> Result<()> {
    if !sparse {
        return clone_repo(url, dest);
    }

    match clone_repo_sparse(url, dest) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("  \u{2022} Falling back to full clone ({})", e);
            let _ = std::fs::remove_dir_all(dest.join(".git"));
            clone_repo(url, dest)
        }
    }
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
        .context("Failed to execute git (clone phase 1)")?;

    if !status.success() {
        bail!("git clone failed — is the repository public and the URL correct?");
    }

    // Phase 2: Initialize sparse-checkout in cone mode
    let init_output = Command::new("git")
        .args(["sparse-checkout", "init", "--cone"])
        .current_dir(dest)
        .output()
        .context("Failed to initialize sparse-checkout")?;

    if !init_output.status.success() {
        let stderr = String::from_utf8_lossy(&init_output.stderr);
        bail!("git sparse-checkout not supported: {}", stderr.trim());
    }

    // Phase 3: Inspect tree to decide what to fetch
    let skills_path = find_case_insensitive_path(dest, "skills")?;
    let has_root_skill = has_path_in_tree(dest, "SKILL.md")?;

    // Phase 4: Set sparse-checkout paths
    let mut paths: Vec<String> = Vec::new();
    if let Some(path) = skills_path {
        paths.push(path);
    }
    if has_root_skill {
        paths.push("SKILL.md".to_string());
    }

    if paths.is_empty() {
        bail!("No SKILL.md found in repository root or skills/ directory");
    }

    let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    let set_output = Command::new("git")
        .args(["sparse-checkout", "set"])
        .args(&path_refs)
        .current_dir(dest)
        .output()
        .context("Failed to set sparse-checkout paths")?;

    if !set_output.status.success() {
        let stderr = String::from_utf8_lossy(&set_output.stderr);
        bail!("Failed to configure sparse-checkout: {}", stderr.trim());
    }

    // Phase 5: Checkout the filtered files (triggers on-demand blob fetch)
    // Use .status() to let git's progress show on the terminal
    eprintln!("  \u{2022} Fetching skill files...");
    let checkout_status = Command::new("git")
        .args(["checkout"])
        .current_dir(dest)
        .status()
        .context("Failed to checkout files")?;

    if !checkout_status.success() {
        bail!("Failed to checkout files after sparse-checkout");
    }

    // Phase 6: Verify working tree is not empty
    let has_files = std::fs::read_dir(dest)
        .context("Failed to read cloned directory")?
        .filter_map(|e| e.ok())
        .any(|e| e.file_name() != ".git");
    if !has_files {
        bail!("Sparse checkout produced empty working tree");
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

/// Find a directory in the git tree using case-insensitive matching.
/// Returns the actual path as stored in the tree (e.g. "Skills" if tree uses uppercase).
fn find_case_insensitive_path(repo_path: &Path, name: &str) -> Result<Option<String>> {
    if has_path_in_tree(repo_path, name)? {
        return Ok(Some(name.to_string()));
    }
    // Try capitalized variant (e.g. "skills" -> "Skills")
    let mut chars = name.chars();
    let capitalized = match chars.next() {
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            let rest: String = chars.collect();
            format!("{}{}", upper, rest)
        }
        None => return Ok(None),
    };
    if has_path_in_tree(repo_path, &capitalized)? {
        Ok(Some(capitalized))
    } else {
        Ok(None)
    }
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

    #[test]
    fn test_find_case_insensitive_path_lowercase() {
        let temp = create_temp_git_repo_with_skills_dir();
        let result = find_case_insensitive_path(temp.path(), "skills").unwrap();
        assert_eq!(result.as_deref(), Some("skills"));
    }

    #[test]
    fn test_find_case_insensitive_path_capitalized() {
        let temp = TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let skills_dir = temp.path().join("Skills").join("my-skill");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(
            skills_dir.join("SKILL.md"),
            "---\nname: test\ndescription: test\n---\n",
        )
        .unwrap();

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

        let result = find_case_insensitive_path(temp.path(), "skills").unwrap();
        assert_eq!(result.as_deref(), Some("Skills"));
    }

    #[test]
    fn test_clone_sparse_with_capitalized_skills() {
        let temp = TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let skills_dir = temp.path().join("Skills").join("my-skill");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(
            skills_dir.join("SKILL.md"),
            "---\nname: test\ndescription: test\n---\n",
        )
        .unwrap();
        fs::write(temp.path().join("large.bin"), vec![0u8; 1024 * 1024]).unwrap();

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
        let path_str = temp.path().to_string_lossy().to_string();
        clone_repo_sparse(&path_str, &dest_path).unwrap();

        assert!(dest_path.join(".git").exists());
        assert!(dest_path.join("Skills").exists());
        // Note: local clones ignore --filter, so large.bin may be present
        // This test verifies the case-insensitive path matching works
    }

    #[test]
    fn test_clone_sparse_case_mismatch_fails() {
        // Repo with "Skills" (capital S) — sparse-checkout with lowercase "skills"
        // won't match, resulting in empty working tree
        let temp = TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let skills_dir = temp.path().join("Skills").join("my-skill");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(
            skills_dir.join("SKILL.md"),
            "---\nname: test\ndescription: test\n---\n",
        )
        .unwrap();

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

        // Simulate the old behavior: hardcode "skills" instead of using find_case_insensitive_path
        let dest = TempDir::new().unwrap();
        let dest_path = dest.path().join("clone");
        // clone first (we can't easily override find_case_insensitive_path, so just
        // verify the function handles it correctly by testing find_case_insensitive_path)
        let path_str = temp.path().to_string_lossy().to_string();
        let found = find_case_insensitive_path(temp.path(), "skills").unwrap();
        assert_eq!(found.as_deref(), Some("Skills"));
        // Now clone with the correct path
        clone_repo_sparse(&path_str, &dest_path).unwrap();
        assert!(dest_path.join("Skills").exists());
    }
}
