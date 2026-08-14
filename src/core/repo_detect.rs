use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::dirs::{NO_SKILL_FOUND_MSG, SKILL_SUBDIRS, SKIP_DIRS};

/// Classification of a cloned repository's skill layout.
/// The rich per-skill data (names, paths, descriptions) lives in
/// `resolve_skills`; this classifier only distinguishes the shapes the
/// fallback (`sole_skill_dir`) needs.
#[derive(Debug, Clone)]
pub(crate) enum RepoType {
    SingleSkillAtRoot,
    SingleSkillInDir { path: PathBuf },
    MultiSkill,
}

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub path: PathBuf,
    pub description: Option<String>,
}

/// Read the repository-root SKILL.md, returning its skill info when present.
fn read_root_skill(repo_path: &Path) -> Result<Option<SkillInfo>> {
    let root_skill = repo_path.join("SKILL.md");
    if !fs::symlink_metadata(&root_skill).is_ok_and(|metadata| metadata.file_type().is_file()) {
        return Ok(None);
    }
    let content = fs::read_to_string(&root_skill)
        .context(format!("Failed to read {}", root_skill.display()))?;
    let frontmatter = crate::core::spec::parse_frontmatter(&content)
        .context("Failed to parse root SKILL.md frontmatter")?;

    Ok(Some(SkillInfo {
        name: frontmatter.name,
        path: repo_path.to_path_buf(),
        description: Some(frontmatter.description),
    }))
}

pub(crate) fn detect_repo_type(repo_path: &Path) -> Result<RepoType> {
    let subdir_skills = find_skills_subdir(repo_path)?;
    let top_level_skills = find_top_level_skills(repo_path)?;

    if read_root_skill(repo_path)?.is_some() {
        if subdir_skills.is_some() || !top_level_skills.is_empty() {
            Ok(RepoType::MultiSkill)
        } else {
            Ok(RepoType::SingleSkillAtRoot)
        }
    } else if let Some(skills) = subdir_skills {
        if skills.len() == 1 {
            let skill = skills.into_iter().next().unwrap();
            Ok(RepoType::SingleSkillInDir { path: skill.path })
        } else {
            Ok(RepoType::MultiSkill)
        }
    } else if !top_level_skills.is_empty() {
        if top_level_skills.len() == 1 {
            let skill = top_level_skills.into_iter().next().unwrap();
            Ok(RepoType::SingleSkillInDir { path: skill.path })
        } else {
            Ok(RepoType::MultiSkill)
        }
    } else {
        bail!(NO_SKILL_FOUND_MSG)
    }
}

/// Return all skills in the repository, root SKILL.md first, then the first
/// populated skills subdirectory, then top-level skill directories.
pub fn resolve_skills(repo_path: &Path) -> Result<Vec<SkillInfo>> {
    let root_skill = read_root_skill(repo_path)?;

    let nested = if let Some(skills) = find_skills_subdir(repo_path)? {
        skills
    } else {
        find_top_level_skills(repo_path)?
    };

    let mut skills = Vec::new();
    if let Some(root) = root_skill {
        skills.push(root);
    }
    skills.extend(nested);

    if skills.is_empty() {
        bail!(NO_SKILL_FOUND_MSG)
    }

    Ok(skills)
}

/// Return the directory of the single skill in the repository, or the
/// repository root when no single skill can be pinned down (legacy fallback
/// for multi-skill repositories without a recorded path).
pub fn sole_skill_dir(repo_path: &Path) -> Result<PathBuf> {
    let candidate = match detect_repo_type(repo_path) {
        Ok(RepoType::SingleSkillAtRoot) => repo_path.to_path_buf(),
        Ok(RepoType::SingleSkillInDir { path, .. }) => path,
        Ok(_) | Err(_) => repo_path.to_path_buf(),
    };
    crate::fs::path_within(repo_path, &candidate)
}

fn find_top_level_skills(repo_path: &Path) -> Result<Vec<SkillInfo>> {
    let mut skills = Vec::new();

    for entry in fs::read_dir(repo_path)? {
        let entry = entry?;
        let path = entry.path();

        if !entry.file_type()?.is_dir() {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && (SKIP_DIRS.contains(&name) || name.starts_with('.'))
        {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if fs::symlink_metadata(&skill_md).is_ok_and(|metadata| metadata.file_type().is_file()) {
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
    if !fs::symlink_metadata(&dir).is_ok_and(|metadata| metadata.file_type().is_dir()) {
        return Ok(None);
    }
    let mut seen = HashSet::new();
    let skills = find_skills_in_dir(&dir, &mut seen)?;
    if skills.is_empty() {
        Ok(None)
    } else {
        Ok(Some(skills))
    }
}

/// The first populated SKILL_SUBDIRS entry, preserving priority order.
fn find_skills_subdir(repo_path: &Path) -> Result<Option<Vec<SkillInfo>>> {
    for subdir in SKILL_SUBDIRS {
        if let Some(skills) = find_skills_in_subdir(repo_path, subdir)? {
            return Ok(Some(skills));
        }
    }
    Ok(None)
}

fn find_skills_in_dir(dir: &Path, seen: &mut HashSet<String>) -> Result<Vec<SkillInfo>> {
    let mut skills = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if !entry.file_type()?.is_dir() {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && (SKIP_DIRS.contains(&name) || name.starts_with('.'))
        {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if fs::symlink_metadata(&skill_md).is_ok_and(|metadata| metadata.file_type().is_file()) {
            let content = fs::read_to_string(&skill_md)
                .context(format!("Failed to read {}", skill_md.display()))?;

            let frontmatter = crate::core::spec::parse_frontmatter(&content)
                .context("Failed to parse SKILL.md frontmatter")?;

            if seen.insert(frontmatter.name.clone()) {
                skills.push(SkillInfo {
                    name: frontmatter.name,
                    path,
                    description: Some(frontmatter.description),
                });
            }
        } else {
            skills.extend(find_skills_in_dir(&path, seen)?);
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
        assert!(matches!(repo_type, RepoType::MultiSkill));
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

    #[cfg(unix)]
    #[test]
    fn test_detection_rejects_symlinked_skill_directories() {
        let temp = TempDir::new().unwrap();
        let outside_temp = TempDir::new().unwrap();
        let outside = outside_temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        make_skill_md(&outside, "escape", "outside");
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        std::os::unix::fs::symlink(&outside, skills_dir.join("escape")).unwrap();

        assert!(detect_repo_type(temp.path()).is_err());
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
        let result = find_skills_in_dir(&skills_dir, &mut HashSet::new()).unwrap();
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
        let result = find_skills_in_dir(&skills_dir, &mut HashSet::new()).unwrap();
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
        assert!(matches!(repo_type, RepoType::MultiSkill));
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
        assert!(matches!(repo_type, RepoType::MultiSkill));
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
            RepoType::SingleSkillInDir { path } => {
                assert!(path.join("SKILL.md").exists());
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
        assert!(matches!(repo_type, RepoType::MultiSkill));
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
        let skills = resolve_skills(temp.path()).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "preferred");
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
        let skills = resolve_skills(temp.path()).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "claude-skill");
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
            RepoType::SingleSkillInDir { path } => {
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
        assert!(matches!(repo_type, RepoType::MultiSkill));
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
        assert!(matches!(repo_type, RepoType::MultiSkill));
    }

    #[test]
    fn test_detect_top_level_single_skill_dir() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("my-skill");
        fs::create_dir_all(&dir).unwrap();
        make_skill_md(&dir, "my-skill", "A skill");
        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::SingleSkillInDir { path } => {
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
        assert!(matches!(repo_type, RepoType::MultiSkill));
    }

    #[test]
    fn test_detect_root_skill_plus_single_subdir_skill_is_multi() {
        let temp = TempDir::new().unwrap();
        make_skill_md(temp.path(), "root-skill", "Root skill");
        let skill_a = temp.path().join("skills/skill-a");
        fs::create_dir_all(&skill_a).unwrap();
        make_skill_md(&skill_a, "skill-a", "Skill A");
        let repo_type = detect_repo_type(temp.path()).unwrap();
        assert!(matches!(repo_type, RepoType::MultiSkill));
    }

    #[test]
    fn test_detect_multi_skill_in_skills_dir_agent_nesting() {
        let temp = TempDir::new().unwrap();
        let claude_skill = temp.path().join("Skills/claude/nmt-chat");
        fs::create_dir_all(&claude_skill).unwrap();
        make_skill_md(&claude_skill, "nmt-chat", "Chat skill");
        let codex_skill = temp.path().join("Skills/codex/nmt-diagnose");
        fs::create_dir_all(&codex_skill).unwrap();
        make_skill_md(&codex_skill, "nmt-diagnose", "Diagnose skill");
        let repo_type = detect_repo_type(temp.path()).unwrap();
        assert!(matches!(repo_type, RepoType::MultiSkill));
    }

    #[test]
    fn test_detect_single_skill_in_skills_dir_agent_nesting() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("Skills/claude/nmt-chat");
        fs::create_dir_all(&skill_dir).unwrap();
        make_skill_md(&skill_dir, "nmt-chat", "Chat skill");
        let repo_type = detect_repo_type(temp.path()).unwrap();
        match repo_type {
            RepoType::SingleSkillInDir { path } => {
                assert!(path.join("SKILL.md").exists());
            }
            other => panic!("Expected SingleSkillInDir, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_skills_root_only() {
        let temp = TempDir::new().unwrap();
        make_skill_md(temp.path(), "root", "Root skill");
        let skills = resolve_skills(temp.path()).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "root");
        assert_eq!(skills[0].path, temp.path());
    }

    #[test]
    fn test_resolve_skills_single_in_dir() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("skills/my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        make_skill_md(&skill_dir, "my-skill", "A skill");
        let skills = resolve_skills(temp.path()).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my-skill");
        assert_eq!(skills[0].path, skill_dir);
    }

    #[test]
    fn test_resolve_skills_multi_prefers_subdir() {
        let temp = TempDir::new().unwrap();
        for name in &["skill-a", "skill-b"] {
            let dir = temp.path().join("skills").join(name);
            fs::create_dir_all(&dir).unwrap();
            make_skill_md(&dir, name, "Test");
        }
        for name in &["top-a", "top-b"] {
            let dir = temp.path().join(name);
            fs::create_dir_all(&dir).unwrap();
            make_skill_md(&dir, name, "Top");
        }
        let skills = resolve_skills(temp.path()).unwrap();
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"skill-a"));
        assert!(names.contains(&"skill-b"));
    }

    #[test]
    fn test_resolve_skills_root_first_then_subdir() {
        let temp = TempDir::new().unwrap();
        make_skill_md(temp.path(), "root", "Root skill");
        let skill_a = temp.path().join("skills/skill-a");
        fs::create_dir_all(&skill_a).unwrap();
        make_skill_md(&skill_a, "skill-a", "Skill A");
        let skills = resolve_skills(temp.path()).unwrap();
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].name, "root");
        assert_eq!(skills[1].name, "skill-a");
    }

    #[test]
    fn test_resolve_skills_none_errors() {
        let temp = TempDir::new().unwrap();
        fs::create_dir_all(temp.path().join("skills")).unwrap();
        assert!(resolve_skills(temp.path()).is_err());
    }

    #[test]
    fn test_sole_skill_dir_root() {
        let temp = TempDir::new().unwrap();
        make_skill_md(temp.path(), "root", "Root skill");
        assert_eq!(
            sole_skill_dir(temp.path()).unwrap(),
            temp.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn test_sole_skill_dir_in_dir() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("skills/my-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        make_skill_md(&skill_dir, "my-skill", "A skill");
        assert_eq!(
            sole_skill_dir(temp.path()).unwrap(),
            skill_dir.canonicalize().unwrap()
        );
    }

    #[test]
    fn test_sole_skill_dir_multi_falls_back_to_root() {
        let temp = TempDir::new().unwrap();
        for name in &["skill-a", "skill-b"] {
            let dir = temp.path().join("skills").join(name);
            fs::create_dir_all(&dir).unwrap();
            make_skill_md(&dir, name, "Test");
        }
        assert_eq!(
            sole_skill_dir(temp.path()).unwrap(),
            temp.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn test_deduplicates_skills_with_same_name_across_agents() {
        let temp = TempDir::new().unwrap();
        // Same skill name in both claude/ and codex/
        let claude_skill = temp.path().join("Skills/claude/nmt-chat");
        fs::create_dir_all(&claude_skill).unwrap();
        make_skill_md(&claude_skill, "nmt-chat", "Chat skill for claude");
        let codex_skill = temp.path().join("Skills/codex/nmt-chat");
        fs::create_dir_all(&codex_skill).unwrap();
        make_skill_md(&codex_skill, "nmt-chat", "Chat skill for codex");
        // Different skill only in codex/
        let codex_diag = temp.path().join("Skills/codex/nmt-diagnose");
        fs::create_dir_all(&codex_diag).unwrap();
        make_skill_md(&codex_diag, "nmt-diagnose", "Diagnose skill");

        let skills = resolve_skills(temp.path()).unwrap();
        assert_eq!(skills.len(), 2);
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"nmt-chat"));
        assert!(names.contains(&"nmt-diagnose"));
    }
}
