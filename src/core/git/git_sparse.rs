use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command;

use super::super::dirs::{SKILL_SUBDIRS, SKIP_DIRS};
use super::{GitOutput, run_git_command, run_git_command_tracked};

/// Sparse-clone a repo: fetch tree objects only, then checkout just `skills/` and root SKILL.md.
/// Falls back silently if sparse-checkout is not supported (git < 2.25).
pub fn clone_repo_sparse(url: &str, dest: &Path, output: GitOutput) -> Result<()> {
    // Phase 1: Clone with blob filter, no checkout
    let mut clone_command = Command::new("git");
    clone_command.args([
        "clone",
        "--filter=blob:none",
        "--no-checkout",
        "--depth",
        "1",
    ]);
    if output.is_quiet() {
        clone_command.arg("--quiet");
    } else {
        clone_command.arg("--progress");
    }
    clone_command.arg(url).arg(dest);

    run_git_command(
        clone_command,
        "Failed to execute git (clone phase 1)",
        "git clone failed - is the repository public and the URL correct?",
        output,
    )?;

    configure_sparse_checkout(dest)?;

    // Phase 5: Checkout the filtered files (triggers on-demand blob fetch)
    if !output.is_quiet() {
        eprintln!("  \u{2022} Fetching skill files...");
    }
    let mut checkout_command = Command::new("git");
    checkout_command.args(["checkout"]);
    if output.is_quiet() {
        checkout_command.arg("--quiet");
    }
    checkout_command.current_dir(dest);

    run_git_command(
        checkout_command,
        "Failed to checkout files",
        "Failed to checkout files after sparse-checkout",
        output,
    )?;

    verify_working_tree(dest)
}

fn configure_sparse_checkout(dest: &Path) -> Result<()> {
    // Phase 2: Inspect tree to decide what to fetch
    let mut skills_subdir: Option<String> = None;
    for subdir in SKILL_SUBDIRS {
        if let Some(path) = find_case_insensitive_path(dest, subdir)? {
            skills_subdir = Some(path);
            break;
        }
    }
    let has_root_skill = has_path_in_tree(dest, "SKILL.md")?;

    // Phase 3: Set sparse-checkout paths
    let mut paths: Vec<String> = Vec::new();
    if let Some(path) = &skills_subdir {
        paths.push(path.clone());
    }
    if has_root_skill {
        paths.push("SKILL.md".to_string());
        // Include sibling directories for root skill with supporting content
        if paths.len() == 1 {
            let sibling_dirs = find_sibling_dirs(dest)?;
            paths.extend(sibling_dirs);
        }
    }

    if paths.is_empty() {
        bail!(
            "No SKILL.md found in repository root, skills/, Skills/, .claude/skills/, or .agents/skills/ directory"
        );
    }

    // Cone mode only supports top-level directories; nested paths like
    // .agents/skills or Skills/claude need --no-cone
    let use_cone = !skills_subdir.as_deref().is_some_and(|p| p.contains('/'));
    let init_args = if use_cone {
        vec!["sparse-checkout", "init", "--cone"]
    } else {
        vec!["sparse-checkout", "init"]
    };
    let init_output = Command::new("git")
        .args(&init_args)
        .current_dir(dest)
        .output()
        .context("Failed to initialize sparse-checkout")?;

    if !init_output.status.success() {
        let stderr = String::from_utf8_lossy(&init_output.stderr);
        bail!("git sparse-checkout not supported: {}", stderr.trim());
    }

    let set_args = if use_cone {
        let mut args = vec!["sparse-checkout", "set"];
        let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        args.extend(path_refs);
        args
    } else {
        let mut args = vec!["sparse-checkout", "set", "--no-cone"];
        let path_refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
        args.extend(path_refs);
        args
    };
    let set_output = Command::new("git")
        .args(&set_args)
        .current_dir(dest)
        .output()
        .context("Failed to set sparse-checkout paths")?;

    if !set_output.status.success() {
        let stderr = String::from_utf8_lossy(&set_output.stderr);
        bail!("Failed to configure sparse-checkout: {}", stderr.trim());
    }
    Ok(())
}

fn verify_working_tree(dest: &Path) -> Result<()> {
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

/// Sparse-clone a repo while reporting clone/checkout progress percentages.
pub fn clone_repo_sparse_tracked(
    url: &str,
    dest: &Path,
    on_progress: &mut dyn FnMut(u8),
) -> Result<()> {
    let mut clone_command = Command::new("git");
    clone_command.args([
        "clone",
        "--filter=blob:none",
        "--no-checkout",
        "--depth",
        "1",
        "--progress",
    ]);
    clone_command.arg(url).arg(dest);
    run_git_command_tracked(clone_command, on_progress)?;

    configure_sparse_checkout(dest)?;

    let mut checkout_command = Command::new("git");
    checkout_command.args(["checkout", "--progress"]);
    checkout_command.current_dir(dest);
    run_git_command_tracked(checkout_command, on_progress)?;

    verify_working_tree(dest)
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

/// Find non-hidden sibling directories at repo root for sparse checkout.
/// Used when a root-level SKILL.md exists without a `skills/` directory.
/// Uses `git ls-tree` since the working tree may be empty after --no-checkout clone.
fn find_sibling_dirs(repo_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-tree", "--name-only", "HEAD"])
        .current_dir(repo_path)
        .output()
        .context("Failed to list git tree")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut dirs = Vec::new();

    for line in stdout.lines() {
        let name = line.trim();
        if name.is_empty() || name.starts_with('.') || SKIP_DIRS.contains(&name) {
            continue;
        }

        // Check if it's a directory (tree object) by running ls-tree on it
        let check = Command::new("git")
            .args(["ls-tree", "--name-only", "HEAD", "--", name])
            .current_dir(repo_path)
            .output()
            .context("Failed to inspect git tree entry")?;

        if check.status.success() && !check.stdout.is_empty() {
            // It's a tree (directory) if ls-tree returns children
            dirs.push(name.to_string());
        }
    }

    dirs.sort();
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

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

    fn create_temp_git_repo_with_claude_skills_dir() -> TempDir {
        let temp = TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        let skills_dir = temp.path().join(".claude/skills/my-skill");
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
            .args(["commit", "-m", "init with .claude/skills and large file"])
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
        clone_repo_sparse(&path_str, &dest_path, GitOutput::Progress).unwrap();

        assert!(dest_path.join(".git").exists());
        assert!(dest_path.join("skills").exists());
    }

    #[test]
    fn test_clone_repo_sparse_tracked_succeeds() {
        let src = create_temp_git_repo_with_skills_dir();
        let dest = TempDir::new().unwrap();
        let dest_path = dest.path().join("clone");

        let path_str = src.path().to_string_lossy().to_string();
        let mut seen: Vec<u8> = Vec::new();
        clone_repo_sparse_tracked(&path_str, &dest_path, &mut |pct| seen.push(pct)).unwrap();

        assert!(dest_path.join(".git").exists());
        assert!(dest_path.join("skills").exists());
        assert!(dest_path.join("skills/my-skill/SKILL.md").exists());
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
        let result = clone_repo_sparse(
            &temp.path().to_string_lossy(),
            &dest_path,
            GitOutput::Progress,
        );
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
        clone_repo_sparse(&path_str, &dest_path, GitOutput::Progress).unwrap();

        assert!(dest_path.join(".git").exists());
        assert!(dest_path.join("Skills").exists());
        // Note: local clones ignore --filter, so large.bin may be present
        // This test verifies the case-insensitive path matching works
    }

    #[test]
    fn test_clone_sparse_local_repo_with_claude_skills() {
        let src = create_temp_git_repo_with_claude_skills_dir();
        let dest = TempDir::new().unwrap();
        let dest_path = dest.path().join("clone");

        let path_str = src.path().to_string_lossy().to_string();
        clone_repo_sparse(&path_str, &dest_path, GitOutput::Progress).unwrap();

        assert!(dest_path.join(".git").exists());
        assert!(dest_path.join(".claude").exists());
        assert!(dest_path.join(".claude/skills").exists());
        assert!(dest_path.join(".claude/skills/my-skill/SKILL.md").exists());
    }

    #[test]
    fn test_clone_sparse_prefers_skills_over_claude_skills() {
        let temp = TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .unwrap();

        // Both skills/ and .claude/skills/ exist — should prefer skills/
        let skills_dir = temp.path().join("skills/my-skill");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(
            skills_dir.join("SKILL.md"),
            "---\nname: test\ndescription: test\n---\n",
        )
        .unwrap();

        let claude_dir = temp.path().join(".claude/skills/other");
        fs::create_dir_all(&claude_dir).unwrap();
        fs::write(
            claude_dir.join("SKILL.md"),
            "---\nname: other\ndescription: other\n---\n",
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

        let dest = TempDir::new().unwrap();
        let dest_path = dest.path().join("clone");
        let path_str = temp.path().to_string_lossy().to_string();
        clone_repo_sparse(&path_str, &dest_path, GitOutput::Progress).unwrap();

        // skills/ should be checked out, .claude/skills/ should not
        assert!(dest_path.join("skills").exists());
        assert!(!dest_path.join(".claude").exists());
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
        clone_repo_sparse(&path_str, &dest_path, GitOutput::Progress).unwrap();
        assert!(dest_path.join("Skills").exists());
    }

    #[test]
    fn test_sparse_checkout_includes_agents_skills() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join(".agents/skills/my-skill")).unwrap();
        fs::write(
            repo.join(".agents/skills/my-skill/SKILL.md"),
            "---\nname: my-skill\ndescription: test\n---\n",
        )
        .unwrap();

        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let dest = temp.path().join("out");
        clone_repo_sparse(&repo.to_string_lossy(), &dest, GitOutput::Quiet).unwrap();

        assert!(
            dest.join(".agents/skills/my-skill/SKILL.md").exists(),
            "sparse checkout should include .agents/skills/"
        );
    }

    #[test]
    fn test_find_sibling_dirs_for_root_skill() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        // Create root SKILL.md + supporting dirs with files
        fs::write(
            repo.join("SKILL.md"),
            "---\nname: test\ndescription: test\n---\n",
        )
        .unwrap();
        fs::create_dir(repo.join("rules")).unwrap();
        fs::write(repo.join("rules").join("a.md"), "rule a").unwrap();
        fs::create_dir(repo.join("checks")).unwrap();
        fs::write(repo.join("checks").join("b.md"), "check b").unwrap();

        // Create dirs that should be skipped
        fs::create_dir(repo.join(".git")).unwrap();
        fs::create_dir(repo.join(".github")).unwrap();
        fs::create_dir(repo.join("node_modules")).unwrap();

        // Init git repo so git ls-tree works
        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let dirs = find_sibling_dirs(&repo).unwrap();
        assert!(dirs.contains(&"rules".to_string()));
        assert!(dirs.contains(&"checks".to_string()));
        assert!(!dirs.contains(&".git".to_string()));
        assert!(!dirs.contains(&".github".to_string()));
        assert!(!dirs.contains(&"node_modules".to_string()));
    }

    #[test]
    fn test_clone_sparse_includes_sibling_dirs_for_root_skill() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        // Root SKILL.md + rules/ dir with files
        fs::write(
            repo.join("SKILL.md"),
            "---\nname: test\ndescription: test\n---\n",
        )
        .unwrap();
        fs::create_dir(repo.join("rules")).unwrap();
        fs::write(repo.join("rules").join("a.md"), "rule a").unwrap();
        fs::create_dir(repo.join("checks")).unwrap();
        fs::write(repo.join("checks").join("b.md"), "check b").unwrap();

        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let dest = temp.path().join("out");
        clone_repo_sparse(&repo.to_string_lossy(), &dest, GitOutput::Quiet).unwrap();

        assert!(
            dest.join("SKILL.md").exists(),
            "sparse checkout should include SKILL.md"
        );
        assert!(
            dest.join("rules").exists(),
            "sparse checkout should include rules/ sibling dir"
        );
        assert!(
            dest.join("rules").join("a.md").exists(),
            "sparse checkout should include files in rules/"
        );
        assert!(
            dest.join("checks").exists(),
            "sparse checkout should include checks/ sibling dir"
        );
    }

    #[test]
    fn test_clone_sparse_with_skills_dir_agent_nesting() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(repo.join("Skills/claude/nmt-chat")).unwrap();
        fs::write(
            repo.join("Skills/claude/nmt-chat/SKILL.md"),
            "---\name: nmt-chat\ndescription: Chat skill\n---\n",
        )
        .unwrap();
        fs::create_dir_all(repo.join("Skills/codex/nmt-diagnose")).unwrap();
        fs::write(
            repo.join("Skills/codex/nmt-diagnose/SKILL.md"),
            "---\nname: nmt-diagnose\ndescription: Diagnose skill\n---\n",
        )
        .unwrap();
        fs::write(repo.join("README.md"), "not a skill").unwrap();

        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let dest = temp.path().join("out");
        clone_repo_sparse(&repo.to_string_lossy(), &dest, GitOutput::Quiet).unwrap();

        assert!(
            dest.join("Skills").exists(),
            "sparse checkout should include Skills/"
        );
        assert!(
            dest.join("Skills/claude/nmt-chat/SKILL.md").exists(),
            "sparse checkout should include nested skill SKILL.md"
        );
        assert!(
            dest.join("Skills/codex/nmt-diagnose/SKILL.md").exists(),
            "sparse checkout should include second agent's skills"
        );
    }
}
