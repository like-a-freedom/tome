use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

use super::skip::{SKILL_SUBDIRS, SKIP_DIRS};

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
    let mut subdir_skills: Option<Vec<SkillInfo>> = None;
    for subdir in SKILL_SUBDIRS {
        if let Some(skills) = find_skills_in_subdir(repo_path, subdir)? {
            subdir_skills = Some(skills);
            break;
        }
    }

    let top_level_skills = find_top_level_skills(repo_path)?;

    let root_skill = repo_path.join("SKILL.md");
    if root_skill.exists() {
        let content = fs::read_to_string(&root_skill)
            .context(format!("Failed to read {}", root_skill.display()))?;
        let frontmatter = crate::core::spec::parse_frontmatter(&content)
            .context("Failed to parse root SKILL.md frontmatter")?;

        let root_info = SkillInfo {
            name: frontmatter.name,
            path: repo_path.to_path_buf(),
            description: Some(frontmatter.description),
        };

        match (&subdir_skills, &top_level_skills) {
            (Some(skills), _) => {
                let mut all_skills = vec![root_info];
                all_skills.extend(skills.iter().cloned());
                Ok(RepoType::MultiSkill(all_skills))
            }
            (None, top_levels) if !top_levels.is_empty() => {
                let mut all_skills = vec![root_info];
                all_skills.extend(top_levels.iter().cloned());
                Ok(RepoType::MultiSkill(all_skills))
            }
            _ => Ok(RepoType::SingleSkillAtRoot),
        }
    } else if let Some(skills) = subdir_skills {
        if skills.len() == 1 {
            let skill = skills.into_iter().next().unwrap();
            Ok(RepoType::SingleSkillInDir {
                name: skill.name,
                path: skill.path,
            })
        } else {
            Ok(RepoType::MultiSkill(skills))
        }
    } else if !top_level_skills.is_empty() {
        if top_level_skills.len() == 1 {
            let skill = top_level_skills.into_iter().next().unwrap();
            Ok(RepoType::SingleSkillInDir {
                name: skill.name,
                path: skill.path,
            })
        } else {
            Ok(RepoType::MultiSkill(top_level_skills))
        }
    } else {
        bail!(
            "No SKILL.md found in repository root, skills/, .claude/skills/, .agents/skills/ directory, or top-level skill directories"
        )
    }
}

fn find_top_level_skills(repo_path: &Path) -> Result<Vec<SkillInfo>> {
    let mut skills = Vec::new();

    for entry in fs::read_dir(repo_path)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && (SKIP_DIRS.contains(&name) || name.starts_with('.'))
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
        }
    }

    Ok(skills)
}

fn find_skills_in_subdir(repo_path: &Path, subdir: &str) -> Result<Option<Vec<SkillInfo>>> {
    let dir = repo_path.join(subdir);
    if !dir.exists() || !dir.is_dir() {
        return Ok(None);
    }
    let skills = find_skills_in_dir(&dir)?;
    if skills.is_empty() {
        Ok(None)
    } else {
        Ok(Some(skills))
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
            && (SKIP_DIRS.contains(&name) || name.starts_with('.'))
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
            skills.extend(find_skills_in_dir(&path)?);
        }
    }

    Ok(skills)
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
    fn test_find_skills_in_dir_nested_categories() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let eng_skill = skills_dir.join("engineering").join("grill-with-docs");
        fs::create_dir_all(&eng_skill).unwrap();
        make_skill_md(&eng_skill, "grill-with-docs", "Grill with docs");
        let prod_skill = skills_dir.join("productivity").join("grill-me");
        fs::create_dir_all(&prod_skill).unwrap();
        make_skill_md(&prod_skill, "grill-me", "Grill me");
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
    fn test_detect_agents_skills_dir() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp
            .path()
            .join(".agents")
            .join("skills")
            .join("impeccable");
        fs::create_dir_all(&skill_dir).unwrap();
        make_skill_md(&skill_dir, "impeccable", "Design frontend interfaces");
        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::SingleSkillInDir { name, .. } => {
                assert_eq!(name, "impeccable");
            }
            _ => panic!("Expected SingleSkillInDir, got {:?}", repo_type),
        }
    }

    #[test]
    fn test_detect_multi_skill_in_agents_skills() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join(".agents/skills");
        for name in &["skill-a", "skill-b"] {
            let dir = skills_dir.join(name);
            fs::create_dir_all(&dir).unwrap();
            make_skill_md(&dir, name, "Test");
        }
        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::MultiSkill(skills) => assert_eq!(skills.len(), 2),
            _ => panic!("Expected MultiSkill, got {:?}", repo_type),
        }
    }

    #[test]
    fn test_detect_prefers_skills_over_claude_skills() {
        let temp = TempDir::new().unwrap();
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
    fn test_detect_prefers_claude_skills_over_agents_skills() {
        let temp = TempDir::new().unwrap();
        let claude_dir = temp.path().join(".claude/skills/my-skill");
        fs::create_dir_all(&claude_dir).unwrap();
        make_skill_md(&claude_dir, "claude-skill", "Should be this one");
        let agents_dir = temp.path().join(".agents/skills/other-skill");
        fs::create_dir_all(&agents_dir).unwrap();
        make_skill_md(&agents_dir, "agents-skill", "Should not be picked");
        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::SingleSkillInDir { name, .. } => assert_eq!(name, "claude-skill"),
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

    #[test]
    fn test_detect_root_skill_plus_subdir_skills_is_multi() {
        let temp = TempDir::new().unwrap();
        make_skill_md(temp.path(), "root-skill", "Root skill");
        let skill_a = temp.path().join("skills/skill-a");
        fs::create_dir_all(&skill_a).unwrap();
        make_skill_md(&skill_a, "skill-a", "Skill A");
        let skill_b = temp.path().join("skills/skill-b");
        fs::create_dir_all(&skill_b).unwrap();
        make_skill_md(&skill_b, "skill-b", "Skill B");
        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::MultiSkill(skills) => {
                assert_eq!(skills.len(), 3);
                let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
                assert!(names.contains(&"root-skill"));
                assert!(names.contains(&"skill-a"));
                assert!(names.contains(&"skill-b"));
            }
            other => panic!("Expected MultiSkill, got {:?}", other),
        }
    }

    #[test]
    fn test_detect_top_level_skill_dirs() {
        let temp = TempDir::new().unwrap();
        for name in &[
            "routeros-firewall",
            "routeros-fundamentals",
            "routeros-scripting",
        ] {
            let dir = temp.path().join(name);
            fs::create_dir_all(&dir).unwrap();
            make_skill_md(&dir, name, &format!("Skill {name}"));
        }
        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::MultiSkill(skills) => {
                assert_eq!(skills.len(), 3);
                let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
                assert!(names.contains(&"routeros-firewall"));
                assert!(names.contains(&"routeros-fundamentals"));
                assert!(names.contains(&"routeros-scripting"));
            }
            other => panic!("Expected MultiSkill, got {:?}", other),
        }
    }

    #[test]
    fn test_detect_top_level_single_skill_dir() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("my-skill");
        fs::create_dir_all(&dir).unwrap();
        make_skill_md(&dir, "my-skill", "A skill");
        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::SingleSkillInDir { name, path } => {
                assert_eq!(name, "my-skill");
                assert!(path.join("SKILL.md").exists());
            }
            other => panic!("Expected SingleSkillInDir, got {:?}", other),
        }
    }

    #[test]
    fn test_detect_root_skill_plus_top_level_dirs_is_multi() {
        let temp = TempDir::new().unwrap();
        make_skill_md(temp.path(), "root-skill", "Root skill");
        for name in &["skill-a", "skill-b"] {
            let dir = temp.path().join(name);
            fs::create_dir_all(&dir).unwrap();
            make_skill_md(&dir, name, "Test");
        }
        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::MultiSkill(skills) => {
                assert_eq!(skills.len(), 3);
                let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
                assert!(names.contains(&"root-skill"));
                assert!(names.contains(&"skill-a"));
                assert!(names.contains(&"skill-b"));
            }
            other => panic!("Expected MultiSkill, got {:?}", other),
        }
    }

    #[test]
    fn test_detect_root_skill_plus_single_subdir_skill_is_multi() {
        let temp = TempDir::new().unwrap();
        make_skill_md(temp.path(), "root-skill", "Root skill");
        let skill_a = temp.path().join("skills/skill-a");
        fs::create_dir_all(&skill_a).unwrap();
        make_skill_md(&skill_a, "skill-a", "Skill A");
        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::MultiSkill(skills) => {
                assert_eq!(skills.len(), 2);
                let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
                assert!(names.contains(&"root-skill"));
                assert!(names.contains(&"skill-a"));
            }
            other => panic!("Expected MultiSkill, got {:?}", other),
        }
    }
}
