//! One-level read-side scan of a skills directory.
//!
//! This is the shared primitive behind `Manifest::rebuild_from_directory`
//! and the list command's untracked-skill scan. It only collects what is on
//! disk; policy (name validation, tracking checks) lives with the callers.

use crate::core::dirs::SKIP_DIRS;
use crate::core::spec;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// A skill found by scanning a skills directory, without any manifest policy.
pub struct ScannedSkill {
    pub dir_name: String,
    pub frontmatter: spec::SkillFrontmatter,
}

/// Scan one level of a skills directory: collect the subdirectories that
/// contain a SKILL.md, skipping dot-directories and SKIP_DIRS entries.
///
/// Directories whose SKILL.md has invalid frontmatter are skipped with a
/// warning on stderr; the parse error is not fatal.
pub fn scan_skills_dir(skills_dir: &Path) -> Result<Vec<ScannedSkill>> {
    let mut scanned = Vec::new();

    if !skills_dir.exists() {
        return Ok(scanned);
    }

    for entry in fs::read_dir(skills_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || SKIP_DIRS.contains(&name) {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }

        let content = fs::read_to_string(&skill_md)
            .context(format!("Failed to read {}", skill_md.display()))?;
        match spec::parse_frontmatter(&content) {
            Ok(frontmatter) => scanned.push(ScannedSkill {
                dir_name: name.to_string(),
                frontmatter,
            }),
            Err(error) => eprintln!(
                "Warning: Skipping {} — invalid frontmatter: {}",
                skill_md.display(),
                error
            ),
        }
    }

    Ok(scanned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_scan_collects_skill_dirs() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        for name in &["skill-a", "skill-b"] {
            let dir = skills_dir.join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("SKILL.md"),
                format!("---\nname: {name}\ndescription: Some desc\n---\n"),
            )
            .unwrap();
        }
        fs::create_dir_all(skills_dir.join("not-a-skill")).unwrap();
        fs::write(skills_dir.join("not-a-skill").join("README.md"), "readme").unwrap();

        let scanned = scan_skills_dir(&skills_dir).unwrap();
        let mut names: Vec<&str> = scanned.iter().map(|s| s.dir_name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["skill-a", "skill-b"]);
        assert_eq!(scanned[0].frontmatter.description, "Some desc");
    }

    #[test]
    fn test_scan_skips_dot_dirs_and_skip_dirs() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        for skipped in &[".hidden", "node_modules", "target"] {
            let dir = skills_dir.join(skipped);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("SKILL.md"),
                "---\nname: Skipped\ndescription: d\n---\n",
            )
            .unwrap();
        }

        let scanned = scan_skills_dir(&skills_dir).unwrap();
        assert!(scanned.is_empty());
    }

    #[test]
    fn test_scan_missing_dir_is_empty() {
        let temp = TempDir::new().unwrap();
        let scanned = scan_skills_dir(&temp.path().join("nope")).unwrap();
        assert!(scanned.is_empty());
    }

    #[test]
    fn test_scan_skips_invalid_frontmatter() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let bad = skills_dir.join("bad-skill");
        fs::create_dir_all(&bad).unwrap();
        fs::write(bad.join("SKILL.md"), "no frontmatter here").unwrap();
        let good = skills_dir.join("good-skill");
        fs::create_dir_all(&good).unwrap();
        fs::write(
            good.join("SKILL.md"),
            "---\nname: good-skill\ndescription: d\n---\n",
        )
        .unwrap();

        let scanned = scan_skills_dir(&skills_dir).unwrap();
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].dir_name, "good-skill");
    }
}
