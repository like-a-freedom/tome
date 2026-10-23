use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

fn parse_latest_percent(s: &str) -> Option<u8> {
    let bytes = s.as_bytes();
    let mut last: Option<u8> = None;
    let mut i = 1usize;
    while i < bytes.len() {
        if bytes[i] == b'%' && bytes[i - 1].is_ascii_digit() {
            let mut start = i - 1;
            while start > 0 && bytes[start - 1].is_ascii_digit() {
                start -= 1;
            }
            if let Ok(pct) = s[start..i].parse::<u8>() {
                last = Some(pct);
            }
        }
        i += 1;
    }
    last
}

pub mod git_sparse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitOutput {
    Progress,
    Quiet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferPhase {
    Clone,
    Checkout,
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

fn run_git_command_tracked(mut command: Command, on_progress: &mut dyn FnMut(u8)) -> Result<()> {
    use std::io::Read;
    use std::process::Stdio;

    command.stdout(Stdio::null()).stderr(Stdio::piped());
    let mut child = command.spawn().context("Failed to execute git")?;
    let mut stderr = child.stderr.take().context("Failed to open git stderr")?;

    let mut chunk = [0u8; 1024];
    let mut acc = String::new();
    let mut last_percent: Option<u8> = None;
    loop {
        let read = stderr
            .read(&mut chunk)
            .context("Failed to read git progress")?;
        if read == 0 {
            break;
        }
        acc.push_str(&String::from_utf8_lossy(&chunk[..read]));
        if let Some(percent) = parse_latest_percent(&acc) {
            if last_percent != Some(percent) {
                last_percent = Some(percent);
                on_progress(percent);
            }
            acc.clear();
        }
    }

    let status = child.wait().context("Failed to wait for git")?;
    if !status.success() {
        let tail = if acc.is_empty() {
            "git command failed".to_string()
        } else {
            acc.trim().to_string()
        };
        bail!("{}", tail);
    }
    Ok(())
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

fn clone_repo_tracked(
    url: &str,
    dest: &Path,
    on_progress: &mut dyn FnMut(TransferPhase, u8),
) -> Result<()> {
    let mut command = Command::new("git");
    command.args(["clone", "--depth", "1", "--progress"]);
    command.arg(url).arg(dest);
    run_git_command_tracked(command, &mut |percent| {
        on_progress(TransferPhase::Clone, percent)
    })
}

pub fn clone_repo_or_sparse_tracked(
    url: &str,
    dest: &Path,
    sparse: bool,
    on_progress: &mut dyn FnMut(TransferPhase, u8),
) -> Result<()> {
    if !sparse {
        return clone_repo_tracked(url, dest, on_progress);
    }

    match git_sparse::clone_repo_sparse_tracked(url, dest, on_progress) {
        Ok(()) => Ok(()),
        Err(sparse_err) => {
            let _ = std::fs::remove_dir_all(dest.join(".git"));
            clone_repo_tracked(url, dest, on_progress).with_context(|| {
                format!("sparse clone failed ({sparse_err}); full clone also failed")
            })
        }
    }
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

/// Materialize a tracked skill that sparse checkout may have omitted.
/// Query the committed tree with a literal pathspec: absence from the working
/// tree alone is never evidence that the source skill was deleted.
pub(crate) fn materialize_tracked_skill(repo_path: &Path, skill_path: &Path) -> Result<()> {
    let canonical_repo = repo_path
        .canonicalize()
        .context("Failed to resolve cloned repository")?;
    let relative = skill_path
        .strip_prefix(repo_path)
        .or_else(|_| skill_path.strip_prefix(&canonical_repo))
        .context("Skill path is outside the cloned repository")?;
    let skill_md = relative.join("SKILL.md");
    let literal = format!(":(literal){}", skill_md.to_string_lossy());
    let entry = git_output(
        repo_path,
        &["ls-tree", "--name-only", "HEAD", "--", &literal],
    )?;
    if entry.is_empty() {
        return Ok(());
    }

    // A full checkout is a safe fallback for custom paths and paths containing
    // sparse-pattern metacharacters, without guessing cone/non-cone patterns.
    git_output(repo_path, &["sparse-checkout", "disable"])
        .context("Failed to materialize the tracked skill from sparse checkout")?;
    if !crate::fs::skill_path_exists(&skill_path.join("SKILL.md"))? {
        anyhow::bail!("Tracked SKILL.md exists in Git but could not be checked out");
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
    use crate::testutil::init_repo_with_identity;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    /// Helper: create a temp git repo with one commit
    fn create_temp_git_repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        init_repo_with_identity(temp.path());

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
        init_repo_with_identity(temp.path());

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
    fn test_parse_latest_percent() {
        assert_eq!(
            parse_latest_percent("Receiving objects:  45% (1234/2741), 1.2 MiB | 1.5 MiB/s"),
            Some(45)
        );
        assert_eq!(
            parse_latest_percent("Receiving objects: 100% (2714/2714), done."),
            Some(100)
        );
        assert_eq!(
            parse_latest_percent("Resolving deltas: 100% (138/138), done."),
            Some(100)
        );
        assert_eq!(parse_latest_percent("Cloning into 'x'..."), None);
    }

    #[test]
    fn test_clone_repo_or_sparse_tracked_succeeds() {
        use std::fs;

        let temp = tempfile::TempDir::new().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(src.join("skills/my-skill")).unwrap();
        fs::write(
            src.join("skills/my-skill/SKILL.md"),
            "---\nname: test\ndescription: test\n---\n",
        )
        .unwrap();
        crate::testutil::init_repo_with_identity(&src);
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(&src)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&src)
            .output()
            .unwrap();

        let dest = temp.path().join("out");
        let mut seen: Vec<u8> = Vec::new();
        let result = clone_repo_or_sparse_tracked(
            src.to_str().unwrap(),
            &dest,
            false,
            &mut |_phase, pct| seen.push(pct),
        );

        assert!(result.is_ok(), "tracked clone should succeed: {:?}", result);
        assert!(dest.join("skills/my-skill/SKILL.md").exists());
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
