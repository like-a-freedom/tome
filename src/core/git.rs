use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

pub mod git_sparse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitOutput {
    Progress,
    Quiet,
}

impl GitOutput {
    fn is_quiet(self) -> bool {
        matches!(self, GitOutput::Quiet)
    }
}

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

pub(crate) fn run_git_command(
    mut command: Command,
    context: &str,
    failure: &str,
    output: GitOutput,
) -> Result<()> {
    if output.is_quiet() {
        let output = command.output().with_context(|| context.to_string())?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("{}: {}", failure, stderr.trim());
        }
        return Ok(());
    }

    let status = command.status().with_context(|| context.to_string())?;
    if !status.success() {
        bail!("{}", failure);
    }
    Ok(())
}

/// Clone a repo, streaming git's progress to the terminal.
pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    clone_repo_with_output(url, dest, GitOutput::Progress)
}

fn clone_repo_with_output(url: &str, dest: &Path, output: GitOutput) -> Result<()> {
    let mut command = Command::new("git");
    command.args(["clone", "--depth", "1"]);
    if output.is_quiet() {
        command.arg("--quiet");
    } else {
        command.arg("--progress");
    }
    command.arg(url).arg(dest);

    run_git_command(
        command,
        "Failed to execute git",
        "git clone failed - is the repository public and the URL correct?",
        output,
    )
}

/// Clone a repo, preferring sparse clone with fallback to full clone when:
/// - sparse-checkout is not supported (git < 2.25)
/// - repository has no `skills/` or root SKILL.md
pub fn clone_repo_or_sparse(url: &str, dest: &Path, sparse: bool, output: GitOutput) -> Result<()> {
    if !sparse {
        return clone_repo_with_output(url, dest, output);
    }

    match git_sparse::clone_repo_sparse(url, dest, output) {
        Ok(()) => Ok(()),
        Err(e) => {
            if !output.is_quiet() {
                eprintln!("  \u{2022} Falling back to full clone ({})", e);
            }
            let _ = std::fs::remove_dir_all(dest.join(".git"));
            clone_repo_with_output(url, dest, output)
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
        let result = clone_repo_quiet("file:///nonexistent/tome-repository.git", &dest);
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
    fn test_clone_repo_or_sparse_accepts_quiet_output() {
        let src = create_temp_git_repo_with_skills_dir();
        let dest = TempDir::new().unwrap();
        let dest_path = dest.path().join("clone");

        let result = clone_repo_or_sparse(
            &src.path().to_string_lossy(),
            &dest_path,
            false,
            GitOutput::Quiet,
        );

        assert!(result.is_ok());
        assert!(
            dest_path
                .join("skills")
                .join("my-skill")
                .join("SKILL.md")
                .exists()
        );
    }
}
