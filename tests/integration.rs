use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn init_git_repo(repo_dir: &std::path::Path) {
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_dir)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(repo_dir)
        .output()
        .unwrap();
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
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

#[test]
fn test_add_single_skill_at_root() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");

    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(
        repo_dir.join("SKILL.md"),
        "---\nname: Root Skill\ndescription: At root\n---\n\n# Root\n",
    )
    .unwrap();
    init_git_repo(&repo_dir);

    let skills_dir = temp.path().join(".agents/skills");
    let repo_url = format!("file://{}", repo_dir.display());

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(repo_url)
        .assert()
        .success()
        .stderr(predicate::str::contains("Installed").and(predicate::str::contains("Root Skill")));

    assert!(skills_dir.join("Root Skill").join("SKILL.md").exists());

    let manifest_path = skills_dir.join(".skills.json");
    assert!(manifest_path.exists());
    let raw = fs::read_to_string(&manifest_path).unwrap();
    assert!(raw.contains("Root Skill"));
}

#[test]
fn test_add_existing_skill_skips() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();

    let existing = skills_dir.join("Existing Skill");
    fs::create_dir_all(&existing).unwrap();
    fs::write(
        existing.join("SKILL.md"),
        "---\nname: Existing Skill\ndescription: Already here\n---\n",
    )
    .unwrap();

    let manifest = skills_dir.join(".skills.json");
    let manifest_data = r#"{
        "version": 1,
        "skills": {
            "Existing Skill": {
                "source": "test/repo", "path": "", "installed": "2026-01-01T00:00:00Z",
                "updated": "2026-01-01T00:00:00Z", "version": "1.0.0",
                "description": "Already here", "commit": "abc", "branch": "main",
                "remote": "https://github.com/test/repo.git"
            }
        }
    }"#;
    fs::write(&manifest, manifest_data).unwrap();

    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(
        repo_dir.join("SKILL.md"),
        "---\nname: Existing Skill\ndescription: New desc\n---\n",
    )
    .unwrap();
    init_git_repo(&repo_dir);

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(format!("file://{}", repo_dir.display()))
        .assert()
        .success()
        .stderr(predicate::str::contains("already installed"));

    // Should NOT overwrite the existing directory
    let content = fs::read_to_string(existing.join("SKILL.md")).unwrap();
    assert!(content.contains("Already here"));
}

#[test]
fn test_add_force_overwrite() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();

    let existing = skills_dir.join("Force Skill");
    fs::create_dir_all(&existing).unwrap();
    fs::write(
        existing.join("SKILL.md"),
        "---\nname: Force Skill\ndescription: Old\n---\n",
    )
    .unwrap();

    let manifest = skills_dir.join(".skills.json");
    let manifest_data = r#"{
        "version": 1,
        "skills": {
            "Force Skill": {
                "source": "test/repo", "path": "", "installed": "2026-01-01T00:00:00Z",
                "updated": "2026-01-01T00:00:00Z", "version": "1.0.0",
                "description": "Old", "commit": "abc", "branch": "main",
                "remote": "https://github.com/test/repo.git"
            }
        }
    }"#;
    fs::write(&manifest, manifest_data).unwrap();

    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(
        repo_dir.join("SKILL.md"),
        "---\nname: Force Skill\ndescription: New desc\n---\n",
    )
    .unwrap();
    init_git_repo(&repo_dir);

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg("--force")
        .arg(format!("file://{}", repo_dir.display()))
        .assert()
        .success();
}

#[test]
fn test_list_empty() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("No skills installed"));
}

#[test]
fn test_list_with_skills() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();

    // Create a tracked skill
    let skill_dir = skills_dir.join("Tracked Skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: Tracked Skill\ndescription: A tracked skill\nversion: 1.0.0\n---\n# Tracked Skill\n",
    ).unwrap();

    let manifest = skills_dir.join(".skills.json");
    let manifest_data = r#"{
        "version": 1,
        "skills": {
            "Tracked Skill": {
                "source": "test/repo", "path": "", "installed": "2026-01-01T00:00:00Z",
                "updated": "2026-01-01T00:00:00Z", "version": "1.0.0",
                "description": "A tracked skill", "commit": "abc", "branch": "main",
                "remote": "https://github.com/test/repo.git"
            }
        }
    }"#;
    fs::write(&manifest, manifest_data).unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("Tracked Skill"))
        .stdout(predicate::str::contains("test/repo"));
}

#[test]
fn test_list_json_output() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();

    let skill_dir = skills_dir.join("JSON Skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: JSON Skill\ndescription: JSON test\n---\n# JSON Skill\n",
    )
    .unwrap();

    let manifest = skills_dir.join(".skills.json");
    let manifest_data = r#"{
        "version": 1,
        "skills": {
            "JSON Skill": {
                "source": "test/repo", "path": "", "installed": "2026-01-01T00:00:00Z",
                "updated": "2026-01-01T00:00:00Z", "version": "1.0.0",
                "description": "JSON test", "commit": "abc", "branch": "main",
                "remote": "https://github.com/test/repo.git"
            }
        }
    }"#;
    fs::write(&manifest, manifest_data).unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("list")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\":\"JSON Skill\""))
        .stdout(predicate::str::contains("\"tracked\":true"));
}

#[test]
fn test_list_shows_untracked_skills() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();

    // Create an untracked (manually placed) skill
    let manual_dir = skills_dir.join("Manual Skill");
    fs::create_dir_all(&manual_dir).unwrap();
    fs::write(
        manual_dir.join("SKILL.md"),
        "---\nname: Manual Skill\ndescription: Placed manually, not in manifest\n---\n# Manual\n",
    )
    .unwrap();

    // Empty manifest — no skills tracked
    let manifest = skills_dir.join(".skills.json");
    let manifest_data = r#"{"version":1,"skills":{}}"#;
    fs::write(&manifest, manifest_data).unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("list")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("Manual Skill"))
        .stdout(predicate::str::contains("\"tracked\":false"));
}

#[test]
fn test_list_rebuilds_missing_manifest() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();

    let skill_dir = skills_dir.join("Orphan Skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: Orphan Skill\ndescription: No manifest here\n---\n# Orphan\n",
    )
    .unwrap();

    // No .skills.json — list should rebuild it
    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("list")
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("Orphan Skill"))
        .stdout(predicate::str::contains("\"tracked\":true"));

    // Verify manifest was created
    assert!(skills_dir.join(".skills.json").exists());
}

#[test]
fn test_remove_tracked_skill() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();

    let skill_dir = skills_dir.join("ToRemove");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: ToRemove\ndescription: Will be removed\n---\n",
    )
    .unwrap();

    let manifest = skills_dir.join(".skills.json");
    let manifest_data = r#"{
        "version": 1,
        "skills": {
            "ToRemove": {
                "source": "test/repo", "path": "", "installed": "2026-01-01T00:00:00Z",
                "updated": "2026-01-01T00:00:00Z", "version": "1.0.0",
                "description": "Will be removed", "commit": "abc", "branch": "main",
                "remote": "https://github.com/test/repo.git"
            }
        }
    }"#;
    fs::write(&manifest, manifest_data).unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("remove")
        .arg("--force")
        .arg("ToRemove")
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed: ToRemove"));

    assert!(!skill_dir.exists());

    // Verify manifest entry removed
    let raw = fs::read_to_string(&manifest).unwrap();
    assert!(!raw.contains("ToRemove"));
}

#[test]
fn test_remove_untracked_skill_error() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();

    let skill_dir = skills_dir.join("ManualRemove");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: ManualRemove\ndescription: Manual\n---\n",
    )
    .unwrap();

    // Empty manifest
    let manifest = skills_dir.join(".skills.json");
    fs::write(&manifest, r#"{"version":1,"skills":{}}"#).unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("remove")
        .arg("--force")
        .arg("ManualRemove")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not installed by tome"));

    // Directory should still exist
    assert!(skill_dir.exists());
}

#[test]
fn test_update_no_skills() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();
    let manifest = skills_dir.join(".skills.json");
    fs::write(&manifest, r#"{"version":1,"skills":{}}"#).unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("update")
        .assert()
        .success()
        .stdout(predicate::str::contains("No skills to update"));
}

#[test]
fn test_update_specific_skill_not_found() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();
    let manifest = skills_dir.join(".skills.json");
    fs::write(&manifest, r#"{"version":1,"skills":{}}"#).unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("update")
        .arg("nonexistent")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}
