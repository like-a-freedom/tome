use crate::core::spec::SkillFrontmatter;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

pub fn make_fm(name: &str) -> SkillFrontmatter {
    SkillFrontmatter {
        name: name.to_string(),
        description: format!("{} desc", name),
        version: Some("1.0.0".into()),
    }
}

/// Create a git repository on disk with a single SKILL.md and return its file:// URL.
pub fn create_git_repo_with_skill(temp: &TempDir, skill_content: &str) -> String {
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(repo_dir.join("SKILL.md"), skill_content).unwrap();
    init_git_repo(&repo_dir);
    format!("file://{}", repo_dir.display())
}

/// Create a git repository with one `skills/<name>/SKILL.md` per name and
/// return its path.
pub fn create_multi_skill_repo(temp: &TempDir, names: &[&str]) -> std::path::PathBuf {
    let repo_dir = temp.path().join("repo");
    for name in names {
        let skill_dir = repo_dir.join("skills").join(name);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name}\nversion: 1.0.0\n---\n"),
        )
        .unwrap();
    }
    init_git_repo(&repo_dir);
    repo_dir
}

/// Initialize an existing directory as a git repo with a local user identity
/// so subsequent `git commit` calls succeed in CI runners (where $HOME points
/// to a per-job temp dir with no global git config). Use this from test
/// helpers instead of calling `git init` directly.
pub fn init_repo_with_identity(repo_dir: &Path) {
    Command::new("git")
        .args(["init", "--initial-branch=main"])
        .current_dir(repo_dir)
        .output()
        .unwrap();
    for (key, value) in [
        ("user.email", "test@example.com"),
        ("user.name", "Test"),
        ("init.defaultBranch", "main"),
    ] {
        Command::new("git")
            .args(["config", key, value])
            .current_dir(repo_dir)
            .output()
            .unwrap();
    }
}

pub fn init_git_repo(repo_dir: &Path) {
    init_repo_with_identity(repo_dir);
    Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo_dir)
        .output()
        .unwrap();
}
