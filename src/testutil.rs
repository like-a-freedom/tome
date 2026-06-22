use crate::core::spec::SkillFrontmatter;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

pub fn make_fm(name: &str) -> SkillFrontmatter {
    SkillFrontmatter {
        name: name.to_string(),
        description: format!("{} desc", name),
        version: Some("1.0.0".into()),
        authors: None,
        tags: None,
        platforms: None,
        license: None,
        repository: None,
        docs: None,
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

pub fn init_git_repo(repo_dir: &Path) {
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_dir)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "t@t.com"])
        .current_dir(repo_dir)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "T"])
        .current_dir(repo_dir)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(repo_dir)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(repo_dir)
        .output()
        .unwrap();
}
