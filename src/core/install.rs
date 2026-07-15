use anyhow::Result;
use std::path::Path;

use crate::core::manifest::{Manifest, SkillEntry};
use crate::core::spec::SkillFrontmatter;
use crate::fs;

/// A skill resolved to its source path, ready for installation.
pub struct ResolvedSkill {
    /// Path to the skill directory containing SKILL.md.
    pub repo_path: std::path::PathBuf,
    /// Parsed frontmatter from SKILL.md.
    pub frontmatter: SkillFrontmatter,
    /// Display label (e.g. "owner/repo").
    pub label: String,
    /// Relative path within the repo (empty if root).
    pub path: String,
    /// Git commit hash (or "file:" for file-URL skills).
    pub commit: String,
    /// Git remote URL (or the file URL).
    pub remote: String,
    /// Git branch name.
    pub branch: String,
}

/// Format a version tag for display: " v1.2.3" or "" if no version.
fn format_version_tag(frontmatter: &SkillFrontmatter) -> String {
    frontmatter
        .version
        .as_deref()
        .filter(|v| !v.is_empty())
        .map(|v| format!(" v{}", v))
        .unwrap_or_default()
}

/// Install resolved skills: copy files, create manifest entries, save.
///
/// Returns a list of display names (e.g. "my-skill v1.0.0") for output.
pub fn install_skills(
    skills: &[ResolvedSkill],
    force: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<InstallResult> {
    fs::ensure_skills_dir(skills_dir)?;
    let mut manifest = Manifest::load(manifest_path)?;

    let mut installed = Vec::new();
    let mut skipped_tracked = Vec::new();
    let mut skipped_existing = Vec::new();

    for skill in skills {
        let dest_dir = skills_dir.join(&skill.frontmatter.name);
        if dest_dir.exists() && !force {
            if manifest.is_tracked(&skill.frontmatter.name) {
                skipped_tracked.push(skill.frontmatter.name.clone());
            } else {
                skipped_existing.push(skill.frontmatter.name.clone());
            }
            continue;
        }

        fs::copy_skill(&skill.repo_path, &dest_dir)?;

        let mut entry = SkillEntry::new(
            &skill.label,
            &skill.path,
            &skill.frontmatter,
            &skill.commit,
            &skill.remote,
        );
        entry.set_branch(&skill.branch);
        manifest.add_skill(&skill.frontmatter.name, entry);

        let version_tag = format_version_tag(&skill.frontmatter);
        installed.push(format!("{}{}", skill.frontmatter.name, version_tag));
    }

    manifest.save(manifest_path)?;

    Ok(InstallResult {
        installed,
        skipped_tracked,
        skipped_existing,
    })
}

/// Result of an install operation.
pub struct InstallResult {
    pub installed: Vec<String>,
    pub skipped_tracked: Vec<String>,
    pub skipped_existing: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;
    use tempfile::TempDir;

    fn make_fm(name: &str) -> SkillFrontmatter {
        testutil::make_fm(name)
    }

    #[test]
    fn test_install_skills_copies_and_registers() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let manifest_path = skills_dir.join(".skills.json");

        let src = temp.path().join("src-skill");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("SKILL.md"),
            "---\nname: my-skill\ndescription: test\nversion: 1.0.0\n---\n",
        )
        .unwrap();

        let fm = make_fm("my-skill");
        let skill = ResolvedSkill {
            repo_path: src,
            frontmatter: fm,
            label: "owner/repo".to_string(),
            path: String::new(),
            commit: "abc1234".to_string(),
            remote: "https://github.com/owner/repo.git".to_string(),
            branch: "main".to_string(),
        };

        let result = install_skills(&[skill], false, &skills_dir, &manifest_path).unwrap();
        assert_eq!(result.installed.len(), 1);
        assert_eq!(result.installed[0], "my-skill v1.0.0");
        assert!(skills_dir.join("my-skill").join("SKILL.md").exists());

        let manifest = Manifest::load(&manifest_path).unwrap();
        assert!(manifest.is_tracked("my-skill"));
    }

    #[test]
    fn test_install_skills_skips_tracked() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let manifest_path = skills_dir.join(".skills.json");

        let src = temp.path().join("src-skill");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("SKILL.md"),
            "---\nname: tracked\ndescription: d\n---\n",
        )
        .unwrap();

        // Pre-populate manifest
        std::fs::create_dir_all(&skills_dir).unwrap();
        let mut manifest = Manifest::load(&manifest_path).unwrap();
        let fm = make_fm("tracked");
        manifest.add_skill(
            "tracked",
            SkillEntry::new("src", "", &fm, "old123", "https://example.com"),
        );
        manifest.save(&manifest_path).unwrap();

        // Create existing dest dir so the skip logic triggers
        std::fs::create_dir_all(skills_dir.join("tracked")).unwrap();

        let skill = ResolvedSkill {
            repo_path: src,
            frontmatter: make_fm("tracked"),
            label: "src".to_string(),
            path: String::new(),
            commit: "new456".to_string(),
            remote: "https://example.com".to_string(),
            branch: "main".to_string(),
        };

        let result = install_skills(&[skill], false, &skills_dir, &manifest_path).unwrap();
        assert!(result.installed.is_empty());
        assert_eq!(result.skipped_tracked.len(), 1);
    }

    #[test]
    fn test_install_skills_force_overwrites() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let manifest_path = skills_dir.join(".skills.json");

        let src = temp.path().join("src-skill");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("SKILL.md"),
            "---\nname: forced\ndescription: new\n---\n",
        )
        .unwrap();

        // Pre-populate manifest
        std::fs::create_dir_all(&skills_dir).unwrap();
        let mut manifest = Manifest::load(&manifest_path).unwrap();
        let fm = make_fm("forced");
        manifest.add_skill(
            "forced",
            SkillEntry::new("old", "", &fm, "old123", "https://example.com"),
        );
        manifest.save(&manifest_path).unwrap();

        let skill = ResolvedSkill {
            repo_path: src,
            frontmatter: make_fm("forced"),
            label: "src".to_string(),
            path: String::new(),
            commit: "new789".to_string(),
            remote: "https://example.com".to_string(),
            branch: "main".to_string(),
        };

        let result = install_skills(&[skill], true, &skills_dir, &manifest_path).unwrap();
        assert_eq!(result.installed.len(), 1);
        assert!(result.skipped_tracked.is_empty());
    }

    #[test]
    fn test_install_skills_empty_list() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        let manifest_path = skills_dir.join(".skills.json");

        let result = install_skills(&[], false, &skills_dir, &manifest_path).unwrap();
        assert!(result.installed.is_empty());
        assert!(result.skipped_tracked.is_empty());
        assert!(result.skipped_existing.is_empty());
    }

    #[test]
    fn test_format_version_tag() {
        let fm = SkillFrontmatter {
            name: "test".to_string(),
            description: "d".to_string(),
            version: Some("1.2.3".to_string()),
            authors: None,
            tags: None,
            platforms: None,
            license: None,
            repository: None,
            docs: None,
        };
        assert_eq!(format_version_tag(&fm), " v1.2.3");

        let fm_no_ver = SkillFrontmatter {
            version: None,
            ..fm.clone()
        };
        assert_eq!(format_version_tag(&fm_no_ver), "");

        let fm_empty = SkillFrontmatter {
            version: Some(String::new()),
            ..fm
        };
        assert_eq!(format_version_tag(&fm_empty), "");
    }
}
