use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

const SKIP_DIRS_IN_SCAN: &[&str] = &[
    ".git",
    ".github",
    ".claude-plugin",
    ".cursor",
    ".agents",
    "node_modules",
    "target",
    "dist",
    "build",
    "__pycache__",
    ".claude",
    ".gemini",
    ".opencode",
    ".codex",
    ".windsurf",
];

#[derive(Debug, Clone)]
pub enum RepoType {
    SingleSkillAtRoot,
    SingleSkillInDir { name: String, path: PathBuf },
    MultiSkill(Vec<SkillInfo>),
}

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
}

pub fn detect_repo_type(repo_path: &Path) -> Result<RepoType> {
    let root_skill = repo_path.join("SKILL.md");
    if root_skill.exists() {
        return Ok(RepoType::SingleSkillAtRoot);
    }

    for subdir in &["skills", ".claude/skills"] {
        if let Some(rt) = check_skills_subdir(repo_path, subdir)? {
            return Ok(rt);
        }
    }

    bail!("No SKILL.md found in repository root, skills/, or .claude/skills/ directory")
}

fn check_skills_subdir(repo_path: &Path, subdir: &str) -> Result<Option<RepoType>> {
    let dir = repo_path.join(subdir);
    if !dir.exists() || !dir.is_dir() {
        return Ok(None);
    }
    let skills = find_skills_in_dir(&dir)?;
    match skills.len() {
        0 => bail!("No SKILL.md files found in {subdir} directory"),
        1 => {
            let skill = skills.into_iter().next().unwrap();
            Ok(Some(RepoType::SingleSkillInDir {
                name: skill.name,
                path: skill.path,
            }))
        }
        _ => Ok(Some(RepoType::MultiSkill(skills))),
    }
}

fn find_skills_in_dir(dir: &Path) -> Result<Vec<SkillInfo>> {
    let mut skills = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && (SKIP_DIRS_IN_SCAN.contains(&name) || name.starts_with('.'))
        {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if skill_md.exists() {
            let content = fs::read_to_string(&skill_md)
                .context(format!("Failed to read {}", skill_md.display()))?;

            let frontmatter = crate::core::spec::parse_frontmatter(&content)
                .context("Failed to parse SKILL.md frontmatter")?;

            skills.push(SkillInfo {
                name: frontmatter.name,
                path,
                description: Some(frontmatter.description),
            });
        } else {
            // Recurse into subdirectories (e.g. skills/{category}/{skill}/SKILL.md)
            skills.extend(find_skills_in_dir(&path)?);
        }
    }

    Ok(skills)
}

pub fn select_skills(skills: &[SkillInfo]) -> Result<Vec<SkillInfo>> {
    if skills.len() == 1 {
        return Ok(vec![skills[0].clone()]);
    }

    let mut items: Vec<crate::tui::SelectItem> = skills
        .iter()
        .map(|s| crate::tui::SelectItem {
            name: s.name.clone(),
            description: s.description.clone().unwrap_or_default(),
            selected: true,
        })
        .collect();

    crate::tui::multi_select(&mut items)?;

    let selected: Vec<SkillInfo> = skills
        .iter()
        .zip(items.iter())
        .filter(|(_, item)| item.selected)
        .map(|(skill, _)| skill.clone())
        .collect();

    if selected.is_empty() {
        bail!("No skills selected");
    }

    Ok(selected)
}

pub fn copy_skill(src: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        fs::remove_dir_all(dest).context("Failed to remove existing skill")?;
    }

    copy_dir_recursive(src, dest).context("Failed to copy skill directory")?;

    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path)?;
        } else {
            fs::copy(&src_path, &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_skill_md(dir: &Path, name: &str, desc: &str) {
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {}\ndescription: {}\n---\n", name, desc),
        )
        .unwrap();
    }

    #[test]
    fn test_detect_single_skill_at_root() {
        let temp = TempDir::new().unwrap();
        make_skill_md(temp.path(), "test", "A test skill");

        let repo_type = detect_repo_type(temp.path()).unwrap();
        assert!(matches!(repo_type, RepoType::SingleSkillAtRoot));
    }

    #[test]
    fn test_detect_single_skill_in_dir() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("skills").join("my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        make_skill_md(&skill_dir, "my-skill", "A test skill");

        let repo_type = detect_repo_type(temp.path()).unwrap();
        assert!(matches!(repo_type, RepoType::SingleSkillInDir { .. }));
    }

    #[test]
    fn test_detect_multi_skill() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        for name in &["skill-a", "skill-b"] {
            let dir = skills_dir.join(name);
            fs::create_dir_all(&dir).unwrap();
            make_skill_md(&dir, name, "Test");
        }

        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::MultiSkill(skills) => assert_eq!(skills.len(), 2),
            _ => panic!("Expected MultiSkill"),
        }
    }

    #[test]
    fn test_no_skill_found() {
        let temp = TempDir::new().unwrap();
        let result = detect_repo_type(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_skips_dot_directories() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let hidden = skills_dir.join(".hidden");
        fs::create_dir_all(&hidden).unwrap();
        make_skill_md(&hidden, "hidden", "Should be skipped");

        let result = detect_repo_type(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_copy_skill_directory() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src-skill");
        fs::create_dir_all(&src).unwrap();
        make_skill_md(&src, "copied", "A copied skill");
        fs::create_dir(src.join("scripts")).unwrap();
        fs::write(src.join("scripts").join("run.sh"), "#!/bin/sh\necho hi").unwrap();

        let dest = temp.path().join("dest-skill");
        copy_skill(&src, &dest).unwrap();

        assert!(dest.join("SKILL.md").exists());
        assert!(dest.join("scripts").join("run.sh").exists());
    }

    #[test]
    fn test_copy_skill_overwrites_existing() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        make_skill_md(&src, "new", "new version");

        let dest = temp.path().join("dest");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("SKILL.md"), "old content").unwrap();

        copy_skill(&src, &dest).unwrap();
        let content = fs::read_to_string(dest.join("SKILL.md")).unwrap();
        assert!(content.contains("new version"));
    }

    #[test]
    fn test_select_skill_single_returns_first() {
        let skills = vec![SkillInfo {
            name: "only".to_string(),
            path: PathBuf::from("/tmp"),
            description: Some("desc".to_string()),
        }];
        let result = select_skills(&skills).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "only");
    }

    #[test]
    fn test_detect_empty_skills_dir_errors() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let result = detect_repo_type(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_skills_dir_with_non_skill_dirs() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("not-a-skill")).unwrap();
        fs::write(
            skills_dir.join("not-a-skill").join("README.md"),
            "not a skill",
        )
        .unwrap();

        let result = detect_repo_type(temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_find_skills_in_dir_skips_skip_dirs() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        for skip in &[".git", "node_modules", "target"] {
            let dir = skills_dir.join(skip);
            fs::create_dir_all(&dir).unwrap();
            make_skill_md(&dir, "skipped", "should not appear");
        }
        let result = find_skills_in_dir(&skills_dir).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_copy_dir_recursive_with_nested_dirs() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(src.join("a/b/c")).unwrap();
        fs::write(src.join("a/b/c/file.txt"), "deep").unwrap();

        let dest = temp.path().join("dest");
        copy_dir_recursive(&src, &dest).unwrap();
        assert!(dest.join("a/b/c/file.txt").exists());
    }

    #[test]
    fn test_find_skills_in_dir_nested_categories() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        // Create mattpocock/skills-like structure:
        // skills/engineering/grill-with-docs/SKILL.md
        // skills/productivity/grill-me/SKILL.md
        let eng_skill = skills_dir.join("engineering").join("grill-with-docs");
        fs::create_dir_all(&eng_skill).unwrap();
        make_skill_md(&eng_skill, "grill-with-docs", "Grill with docs");

        let prod_skill = skills_dir.join("productivity").join("grill-me");
        fs::create_dir_all(&prod_skill).unwrap();
        make_skill_md(&prod_skill, "grill-me", "Grill me");

        // Category READMEs should not break anything
        fs::write(skills_dir.join("engineering").join("README.md"), "readme").unwrap();

        let result = find_skills_in_dir(&skills_dir).unwrap();
        assert_eq!(result.len(), 2);
        let names: Vec<&str> = result.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"grill-with-docs"));
        assert!(names.contains(&"grill-me"));
    }

    #[test]
    fn test_detect_multi_skill_nested_categories() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        let skill_a = skills_dir.join("engineering").join("skill-a");
        fs::create_dir_all(&skill_a).unwrap();
        make_skill_md(&skill_a, "skill-a", "Skill A");

        let skill_b = skills_dir.join("productivity").join("skill-b");
        fs::create_dir_all(&skill_b).unwrap();
        make_skill_md(&skill_b, "skill-b", "Skill B");

        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::MultiSkill(skills) => {
                assert_eq!(skills.len(), 2);
                let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
                assert!(names.contains(&"skill-a"));
                assert!(names.contains(&"skill-b"));
            }
            _ => panic!("Expected MultiSkill"),
        }
    }

    #[test]
    fn test_detect_single_skill_in_claude_skills() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join(".claude/skills/my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        make_skill_md(&skill_dir, "my-skill", "A test skill");

        let repo_type = detect_repo_type(temp.path()).unwrap();
        assert!(matches!(repo_type, RepoType::SingleSkillInDir { .. }));
    }

    #[test]
    fn test_detect_multi_skill_in_claude_skills() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join(".claude/skills");

        for name in &["skill-a", "skill-b"] {
            let dir = skills_dir.join(name);
            fs::create_dir_all(&dir).unwrap();
            make_skill_md(&dir, name, "Test");
        }

        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::MultiSkill(skills) => assert_eq!(skills.len(), 2),
            _ => panic!("Expected MultiSkill"),
        }
    }

    #[test]
    fn test_detect_prefers_skills_over_claude_skills() {
        let temp = TempDir::new().unwrap();

        // Both skills/ and .claude/skills/ exist — should prefer skills/
        let skill_dir = temp.path().join("skills/my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        make_skill_md(&skill_dir, "preferred", "Should be this one");

        let claude_dir = temp.path().join(".claude/skills/other");
        fs::create_dir_all(&claude_dir).unwrap();
        make_skill_md(&claude_dir, "other", "Should not be picked");

        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::SingleSkillInDir { name, .. } => assert_eq!(name, "preferred"),
            _ => panic!("Expected SingleSkillInDir"),
        }
    }

    #[test]
    fn test_detect_single_skill_nested_category() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        let skill = skills_dir.join("engineering").join("my-skill");
        fs::create_dir_all(&skill).unwrap();
        make_skill_md(&skill, "my-skill", "A nested skill");

        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::SingleSkillInDir { name, path } => {
                assert_eq!(name, "my-skill");
                assert!(path.join("SKILL.md").exists());
            }
            _ => panic!("Expected SingleSkillInDir"),
        }
    }
}
