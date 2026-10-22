use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::process::Command as ProcessCommand;
use tempfile::TempDir;

mod common;

#[test]
fn test_add_single_skill_in_claude_skills_dir() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");

    fs::create_dir_all(repo_dir.join(".claude/skills/my-skill")).unwrap();
    fs::write(
        repo_dir.join(".claude/skills/my-skill/SKILL.md"),
        "---\nname: ClaudeSkill\ndescription: in .claude/skills\n---\n",
    )
    .unwrap();
    fs::write(repo_dir.join("large.bin"), vec![0u8; 1024 * 1024]).unwrap();
    common::init_git_repo(&repo_dir);

    let skills_dir = temp.path().join(".agents/skills");
    let repo_url = format!("file://{}", repo_dir.display());

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(&repo_url)
        .assert()
        .success()
        .stderr(predicate::str::contains("ClaudeSkill"));

    assert!(skills_dir.join("ClaudeSkill").join("SKILL.md").exists());
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
    common::init_git_repo(&repo_dir);

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
    common::init_git_repo(&repo_dir);

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
    common::init_git_repo(&repo_dir);

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

#[test]
fn test_update_details_require_verbose() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");

    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(
        repo_dir.join("SKILL.md"),
        "---\nname: CompactSkill\ndescription: compact update test\n---\n",
    )
    .unwrap();
    common::init_git_repo(&repo_dir);

    let skills_dir = temp.path().join(".agents/skills");
    let repo_url = format!("file://{}", repo_dir.display());

    let mut add = Command::cargo_bin("tome").unwrap();
    add.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(&repo_url)
        .assert()
        .success();

    let mut compact = Command::cargo_bin("tome").unwrap();
    compact
        .env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("update")
        .assert()
        .success()
        .stdout(predicate::str::contains("CompactSkill").not());

    let mut verbose = Command::cargo_bin("tome").unwrap();
    verbose
        .env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .args(["update", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Already up to date: CompactSkill"));
}

#[test]
fn test_update_changed_git_skill_through_cli() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    fs::create_dir_all(&repo_dir).unwrap();
    fs::write(
        repo_dir.join("SKILL.md"),
        "---\nname: ChangedSkill\ndescription: old\n---\nold body\n",
    )
    .unwrap();
    common::init_git_repo(&repo_dir);

    let skills_dir = temp.path().join(".agents/skills");
    let repo_url = format!("file://{}", repo_dir.display());
    let mut add = Command::cargo_bin("tome").unwrap();
    add.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(&repo_url)
        .assert()
        .success();

    fs::write(
        repo_dir.join("SKILL.md"),
        "---\nname: ChangedSkill\ndescription: new\n---\nnew body\n",
    )
    .unwrap();
    ProcessCommand::new("git")
        .args(["add", "SKILL.md"])
        .current_dir(&repo_dir)
        .status()
        .unwrap();
    ProcessCommand::new("git")
        .args(["commit", "-m", "update skill"])
        .current_dir(&repo_dir)
        .status()
        .unwrap();

    let mut update = Command::cargo_bin("tome").unwrap();
    update
        .env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .args(["update", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated: ChangedSkill"));

    let installed = fs::read_to_string(skills_dir.join("ChangedSkill/SKILL.md")).unwrap();
    assert!(installed.contains("new body"));
}

#[test]
fn test_update_installs_new_skill_added_to_tracked_repo() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    let alpha_dir = repo_dir.join("skills/alpha");
    fs::create_dir_all(&alpha_dir).unwrap();
    fs::write(
        alpha_dir.join("SKILL.md"),
        "---\nname: AlphaSkill\ndescription: first skill\n---\n",
    )
    .unwrap();
    common::init_git_repo(&repo_dir);

    let skills_dir = temp.path().join(".agents/skills");
    let repo_url = format!("file://{}", repo_dir.display());
    let mut add = Command::cargo_bin("tome").unwrap();
    add.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(&repo_url)
        .assert()
        .success();

    let beta_dir = repo_dir.join("skills/beta");
    fs::create_dir_all(&beta_dir).unwrap();
    fs::write(
        beta_dir.join("SKILL.md"),
        "---\nname: BetaSkill\ndescription: new skill\n---\n",
    )
    .unwrap();
    ProcessCommand::new("git")
        .args(["add", "skills/beta"])
        .current_dir(&repo_dir)
        .status()
        .unwrap();
    ProcessCommand::new("git")
        .args(["commit", "-m", "add beta skill"])
        .current_dir(&repo_dir)
        .status()
        .unwrap();

    let mut update = Command::cargo_bin("tome").unwrap();
    update
        .env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .args(["update", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated: BetaSkill"));

    assert!(skills_dir.join("BetaSkill/SKILL.md").exists());
    assert!(skills_dir.join("AlphaSkill/SKILL.md").exists());

    let mut list = Command::cargo_bin("tome").unwrap();
    list.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\":\"BetaSkill\""))
        .stdout(predicate::str::contains("\"tracked\":true"));

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(skills_dir.join(".skills.json")).unwrap())
            .unwrap();
    let beta = &manifest["skills"]["BetaSkill"];
    assert_eq!(beta["source"], repo_url);
    assert_eq!(beta["remote"], repo_url);
    assert_eq!(beta["path"], "skills/beta");
    assert_eq!(beta["branch"], "main");
    assert!(!beta["commit"].as_str().unwrap().is_empty());
}

#[test]
fn test_update_does_not_overwrite_untracked_skill_with_same_name() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    let alpha_dir = repo_dir.join("skills/alpha");
    fs::create_dir_all(&alpha_dir).unwrap();
    fs::write(
        alpha_dir.join("SKILL.md"),
        "---\nname: AlphaSkill\ndescription: first skill\n---\n",
    )
    .unwrap();
    common::init_git_repo(&repo_dir);

    let skills_dir = temp.path().join(".agents/skills");
    let repo_url = format!("file://{}", repo_dir.display());
    let mut add = Command::cargo_bin("tome").unwrap();
    add.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(&repo_url)
        .assert()
        .success();

    let beta_dir = repo_dir.join("skills/beta");
    fs::create_dir_all(&beta_dir).unwrap();
    fs::write(
        beta_dir.join("SKILL.md"),
        "---\nname: BetaSkill\ndescription: new skill\n---\n",
    )
    .unwrap();
    ProcessCommand::new("git")
        .args(["add", "skills/beta"])
        .current_dir(&repo_dir)
        .status()
        .unwrap();
    ProcessCommand::new("git")
        .args(["commit", "-m", "add beta skill"])
        .current_dir(&repo_dir)
        .status()
        .unwrap();

    fs::create_dir_all(skills_dir.join("BetaSkill")).unwrap();
    fs::write(
        skills_dir.join("BetaSkill/SKILL.md"),
        "---\nname: BetaSkill\ndescription: manually placed\n---\nmanual content\n",
    )
    .unwrap();

    let mut update = Command::cargo_bin("tome").unwrap();
    update
        .env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .args(["update", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated: BetaSkill").not());

    let content = fs::read_to_string(skills_dir.join("BetaSkill/SKILL.md")).unwrap();
    assert!(content.contains("manual content"));
    assert!(!content.contains("new skill"));

    let mut list = Command::cargo_bin("tome").unwrap();
    list.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .args(["list", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\":\"BetaSkill\""))
        .stdout(predicate::str::contains("\"tracked\":false"));
}

#[test]
fn test_update_specific_skill_does_not_sync_new_repo_skills() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    let alpha_dir = repo_dir.join("skills/alpha");
    fs::create_dir_all(&alpha_dir).unwrap();
    fs::write(
        alpha_dir.join("SKILL.md"),
        "---\nname: AlphaSkill\ndescription: first skill\n---\n",
    )
    .unwrap();
    common::init_git_repo(&repo_dir);

    let skills_dir = temp.path().join(".agents/skills");
    let repo_url = format!("file://{}", repo_dir.display());
    let mut add = Command::cargo_bin("tome").unwrap();
    add.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(&repo_url)
        .assert()
        .success();

    let beta_dir = repo_dir.join("skills/beta");
    fs::create_dir_all(&beta_dir).unwrap();
    fs::write(
        beta_dir.join("SKILL.md"),
        "---\nname: BetaSkill\ndescription: new skill\n---\n",
    )
    .unwrap();
    ProcessCommand::new("git")
        .args(["add", "skills/beta"])
        .current_dir(&repo_dir)
        .status()
        .unwrap();
    ProcessCommand::new("git")
        .args(["commit", "-m", "add beta skill"])
        .current_dir(&repo_dir)
        .status()
        .unwrap();

    let mut update = Command::cargo_bin("tome").unwrap();
    update
        .env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .args(["update", "AlphaSkill", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated: AlphaSkill"))
        .stdout(predicate::str::contains("Updated: BetaSkill").not());

    assert!(skills_dir.join("AlphaSkill/SKILL.md").exists());
    assert!(!skills_dir.join("BetaSkill").exists());
}

#[test]
fn test_update_removes_skill_deleted_from_custom_source_path() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");
    let source_skill = repo_dir.join("skills/custom");
    fs::create_dir_all(&source_skill).unwrap();
    fs::write(
        source_skill.join("SKILL.md"),
        "---\nname: CustomPathSkill\ndescription: custom path\n---\n",
    )
    .unwrap();
    common::init_git_repo(&repo_dir);

    let skills_dir = temp.path().join(".agents/skills");
    let repo_url = format!("file://{}", repo_dir.display());
    let mut add = Command::cargo_bin("tome").unwrap();
    add.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .args(["add", "--path", "skills/custom", &repo_url])
        .assert()
        .success();

    fs::remove_dir_all(&source_skill).unwrap();
    ProcessCommand::new("git")
        .args(["add", "--all"])
        .current_dir(&repo_dir)
        .status()
        .unwrap();
    ProcessCommand::new("git")
        .args(["commit", "-m", "delete custom skill"])
        .current_dir(&repo_dir)
        .status()
        .unwrap();

    let mut update = Command::cargo_bin("tome").unwrap();
    update
        .env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .args(["update", "--no-sparse", "--verbose"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Removed CustomPathSkill"));

    assert!(!skills_dir.join("CustomPathSkill").exists());
}

#[test]
fn test_update_changed_file_url_skill_through_cli() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("SKILL.md");
    fs::write(
        &source,
        "---\nname: FileSkill\ndescription: old\n---\nold body\n",
    )
    .unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    let file_url = format!("file://{}", source.display());

    let mut add = Command::cargo_bin("tome").unwrap();
    add.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(&file_url)
        .assert()
        .success();

    fs::write(
        &source,
        "---\nname: FileSkill\ndescription: new\n---\nnew body\n",
    )
    .unwrap();

    let mut update = Command::cargo_bin("tome").unwrap();
    update
        .env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .args(["update", "--verbose"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Updated: FileSkill (file)"));

    let installed = fs::read_to_string(skills_dir.join("FileSkill/SKILL.md")).unwrap();
    assert!(installed.contains("new body"));
}

#[test]
fn test_add_sparse_clone_with_skills_dir() {
    let temp = TempDir::new().unwrap();
    let repo_dir = temp.path().join("repo");

    fs::create_dir_all(repo_dir.join("skills/my-skill")).unwrap();
    fs::write(
        repo_dir.join("skills/my-skill/SKILL.md"),
        "---\nname: IntSparseSkill\ndescription: integration test\n---\n",
    )
    .unwrap();
    fs::write(repo_dir.join("large.bin"), vec![0u8; 1024 * 1024]).unwrap();
    common::init_git_repo(&repo_dir);

    let skills_dir = temp.path().join(".agents/skills");
    let repo_url = format!("file://{}", repo_dir.display());

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(&repo_url)
        .assert()
        .success()
        .stderr(predicate::str::contains("IntSparseSkill"));

    assert!(skills_dir.join("IntSparseSkill").join("SKILL.md").exists());
}

#[test]
fn test_remove_by_source_label() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();

    // Create two skills from the same "repo"
    for name in &["git-commit", "code-review"] {
        let dir = skills_dir.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {}\ndescription: from same repo\n---\n", name),
        )
        .unwrap();
    }

    let manifest = skills_dir.join(".skills.json");
    let manifest_data = r#"{
        "version": 1,
        "skills": {
            "git-commit": {
                "source": "leonardomso/rust-skills", "path": "", "installed": "2026-01-01T00:00:00Z",
                "updated": "2026-01-01T00:00:00Z", "version": "1.0.0",
                "description": "from same repo", "commit": "abc", "branch": "main",
                "remote": "https://github.com/leonardomso/rust-skills.git"
            },
            "code-review": {
                "source": "leonardomso/rust-skills", "path": "", "installed": "2026-01-01T00:00:00Z",
                "updated": "2026-01-01T00:00:00Z", "version": "1.0.0",
                "description": "from same repo", "commit": "abc", "branch": "main",
                "remote": "https://github.com/leonardomso/rust-skills.git"
            }
        }
    }"#;
    fs::write(&manifest, manifest_data).unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("remove")
        .arg("--force")
        .arg("leonardomso/rust-skills")
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed: git-commit"))
        .stdout(predicate::str::contains("Removed: code-review"));

    assert!(!skills_dir.join("git-commit").exists());
    assert!(!skills_dir.join("code-review").exists());

    let raw = fs::read_to_string(&manifest).unwrap();
    assert!(!raw.contains("git-commit"));
    assert!(!raw.contains("code-review"));
}

#[test]
fn test_remove_by_url() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(skills_dir.join("my-skill")).unwrap();
    fs::write(
        skills_dir.join("my-skill").join("SKILL.md"),
        "---\nname: my-skill\ndescription: test\n---\n",
    )
    .unwrap();

    let manifest = skills_dir.join(".skills.json");
    let manifest_data = r#"{
        "version": 1,
        "skills": {
            "my-skill": {
                "source": "owner/repo", "path": "", "installed": "2026-01-01T00:00:00Z",
                "updated": "2026-01-01T00:00:00Z", "version": "1.0.0",
                "description": "test", "commit": "abc", "branch": "main",
                "remote": "https://github.com/owner/repo.git"
            }
        }
    }"#;
    fs::write(&manifest, manifest_data).unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("remove")
        .arg("--force")
        .arg("https://github.com/owner/repo")
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed: my-skill"));

    assert!(!skills_dir.join("my-skill").exists());
}

#[test]
fn test_remove_file_url_source() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("SKILL.md");
    fs::write(
        &source,
        "---\nname: FileRemove\ndescription: file removal\n---\n",
    )
    .unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    let file_url = format!("file://{}", source.display());

    let mut add = Command::cargo_bin("tome").unwrap();
    add.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("add")
        .arg(&file_url)
        .assert()
        .success();

    let mut remove = Command::cargo_bin("tome").unwrap();
    remove
        .env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("remove")
        .arg("--force")
        .arg(&file_url)
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed: FileRemove"));

    assert!(!skills_dir.join("FileRemove").exists());
}

#[test]
fn test_remove_by_source_not_found() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();
    fs::write(
        skills_dir.join(".skills.json"),
        r#"{"version":1,"skills":{}}"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("remove")
        .arg("--force")
        .arg("nonexistent/repo")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not installed by tome"));
}

#[test]
fn test_export_tracked_skills_through_cli() {
    let temp = TempDir::new().unwrap();
    let skills_dir = temp.path().join(".agents/skills");
    fs::create_dir_all(&skills_dir).unwrap();
    fs::write(
        skills_dir.join(".skills.json"),
        r#"{
            "version": 1,
            "skills": {
                "exported-skill": {
                    "source": "owner/repo", "path": "skills/exported", "installed": "2026-01-01T00:00:00Z",
                    "updated": "2026-01-01T00:00:00Z", "version": "1.0.0",
                    "description": "export test", "commit": "abc", "branch": "main",
                    "remote": "https://github.com/owner/repo.git"
                }
            }
        }"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("tome").unwrap();
    cmd.env("SKILLS_DIR", skills_dir.to_str().unwrap())
        .arg("export")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "tome add owner/repo --path skills/exported",
        ));
}

#[test]
fn scalable_list_plain_and_json_keep_all_500_skills_in_pipes() {
    let temp = TempDir::new().unwrap();
    for i in 0..500 {
        let name = format!("skill-{i:04}");
        let dir = temp.path().join(&name);
        fs::create_dir(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Example\n---\n"),
        )
        .unwrap();
    }
    let output = Command::cargo_bin("tome")
        .unwrap()
        .env("SKILLS_DIR", temp.path())
        .args(["list", "--plain"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let table = String::from_utf8(output.stdout).unwrap();
    assert!(table.contains("skill-0000") && table.contains("skill-0499"));
    assert!(!table.contains("\x1b[?1049"));
    let output = Command::cargo_bin("tome")
        .unwrap()
        .env("SKILLS_DIR", temp.path())
        .args(["list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 500);
}

#[test]
fn scalable_remove_without_target_in_pipe_is_actionable_error() {
    let temp = TempDir::new().unwrap();
    Command::cargo_bin("tome")
        .unwrap()
        .env("SKILLS_DIR", temp.path())
        .arg("remove")
        .assert()
        .failure()
        .stderr(predicate::str::contains("terminal"));
}

#[test]
fn scalable_interactive_commands_reject_pipes_before_work() {
    let temp = TempDir::new().unwrap();
    for command in ["update", "export"] {
        Command::cargo_bin("tome")
            .unwrap()
            .env("SKILLS_DIR", temp.path())
            .args([command, "--interactive"])
            .assert()
            .failure()
            .stderr(predicate::str::contains("terminal"));
    }
}

#[test]
fn add_root_skill_rejects_wrong_explicit_selector() {
    let temp = TempDir::new().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(
        repo.join("SKILL.md"),
        "---\nname: actual\ndescription: root skill\n---\n",
    )
    .unwrap();
    common::init_git_repo(&repo);
    let git_config = temp.path().join("gitconfig");
    fs::write(
        &git_config,
        format!(
            "[url \"file://{}\"]\n    insteadOf = https://github.com/audit/repo.git\n",
            repo.display()
        ),
    )
    .unwrap();
    let skills = temp.path().join("skills");
    Command::cargo_bin("tome")
        .unwrap()
        .env("SKILLS_DIR", &skills)
        .env("GIT_CONFIG_GLOBAL", &git_config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["add", "audit/repo@wrong", "--no-sparse"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Requested skill"));
    assert!(!skills.join("actual").exists());
}

#[test]
fn failed_update_returns_failure_and_preserves_installed_skill() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("SKILL.md");
    let content = "---\nname: stable\ndescription: old\n---\n";
    fs::write(&source, content).unwrap();
    let skills = temp.path().join("skills");
    Command::cargo_bin("tome")
        .unwrap()
        .env("SKILLS_DIR", &skills)
        .args(["add", &format!("file://{}", source.display())])
        .assert()
        .success();
    fs::write(
        &source,
        "---\nname: stable\n---\ninvalid missing description",
    )
    .unwrap();
    Command::cargo_bin("tome")
        .unwrap()
        .env("SKILLS_DIR", &skills)
        .arg("update")
        .assert()
        .failure();
    assert_eq!(
        fs::read_to_string(skills.join("stable/SKILL.md")).unwrap(),
        content
    );
}

#[test]
fn cli_git_lifecycle_preserves_scope_and_reconciles_hundreds_of_skills() {
    const EXTRA_SKILLS: usize = 200;
    for sparse in [false, true] {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        let original = repo.join("skills/original");
        fs::create_dir_all(original.join("scripts")).unwrap();
        let initial = "---\nname: original\ndescription: initial\nversion: 1\n---\n";
        let updated = "---\nname: original\ndescription: updated\nversion: 2\n---\n";
        fs::write(original.join("SKILL.md"), initial).unwrap();
        fs::write(original.join("scripts/old.txt"), "old asset").unwrap();
        common::init_git_repo(&repo);
        let url = format!("file://{}", repo.display());
        let skills = temp.path().join("skills");
        let manifest_path = skills.join(".skills.json");
        let run = |args: &[&str]| {
            let mut command = Command::cargo_bin("tome").unwrap();
            command.env("SKILLS_DIR", &skills).args(args);
            if !sparse && matches!(args.first(), Some(&"add" | &"update")) {
                command.arg("--no-sparse");
            }
            command.assert()
        };
        let read_manifest = || -> serde_json::Value {
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap()
        };
        let commit = || {
            for args in [
                ["add", "-A"].as_slice(),
                ["commit", "-m", "change"].as_slice(),
            ] {
                let output = ProcessCommand::new("git")
                    .current_dir(&repo)
                    .args(args)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            let output = ProcessCommand::new("git")
                .current_dir(&repo)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap();
            assert!(output.status.success());
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        };

        run(&["add", &url]).success();
        assert_eq!(read_manifest()["skills"].as_object().unwrap().len(), 1);
        fs::write(skills.join("original/SKILL.md"), "local edit").unwrap();
        run(&["add", &url]).success();
        assert_eq!(
            fs::read_to_string(skills.join("original/SKILL.md")).unwrap(),
            "local edit"
        );
        run(&["add", &url, "--force"]).success();
        assert_eq!(
            fs::read_to_string(skills.join("original/SKILL.md")).unwrap(),
            initial
        );

        fs::write(original.join("SKILL.md"), updated).unwrap();
        fs::remove_file(original.join("scripts/old.txt")).unwrap();
        fs::write(original.join("scripts/new.txt"), "new asset").unwrap();
        for index in 0..EXTRA_SKILLS {
            let name = format!("extra-{index:03}");
            let dir = repo.join("skills").join(&name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: new skill\n---\n"),
            )
            .unwrap();
        }
        let new_commit = commit();
        run(&["update", "original"]).success();
        assert_eq!(
            read_manifest()["skills"].as_object().unwrap().len(),
            1,
            "targeted update must not discover unrelated skills"
        );
        assert_eq!(
            fs::read_to_string(skills.join("original/SKILL.md")).unwrap(),
            updated
        );
        assert!(!skills.join("original/scripts/old.txt").exists());
        assert_eq!(
            fs::read_to_string(skills.join("original/scripts/new.txt")).unwrap(),
            "new asset"
        );

        run(&["update"]).success();
        let after_sync = read_manifest();
        assert_eq!(
            after_sync["skills"].as_object().unwrap().len(),
            EXTRA_SKILLS + 1
        );
        for (name, entry) in after_sync["skills"].as_object().unwrap() {
            assert_eq!(entry["commit"], new_commit);
            assert!(skills.join(name).join("SKILL.md").is_file());
        }
        run(&["update"]).success();
        assert_eq!(
            read_manifest(),
            after_sync,
            "unchanged update must be idempotent"
        );

        fs::remove_dir_all(&original).unwrap();
        commit();
        run(&["update"]).success();
        assert!(!skills.join("original").exists());
        assert!(read_manifest()["skills"].get("original").is_none());
        assert_eq!(
            read_manifest()["skills"].as_object().unwrap().len(),
            EXTRA_SKILLS
        );

        let before_failure = read_manifest();
        let unavailable = temp.path().join("unavailable");
        fs::rename(&repo, &unavailable).unwrap();
        run(&["update"]).failure();
        assert_eq!(read_manifest(), before_failure);
        for name in before_failure["skills"].as_object().unwrap().keys() {
            assert!(skills.join(name).join("SKILL.md").is_file());
        }
        fs::rename(&unavailable, &repo).unwrap();
        run(&["remove", &url, "--force"]).success();
        assert!(read_manifest()["skills"].as_object().unwrap().is_empty());
        for name in before_failure["skills"].as_object().unwrap().keys() {
            assert!(!skills.join(name).exists());
        }
        assert!(fs::read_dir(&skills).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".tome-transaction-")
        }));
        run(&["remove", &url, "--force"]).failure();
    }
}
