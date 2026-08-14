use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::core::spec::SkillFrontmatter;

/// Commit value recorded for file:// sources, which have no git history.
pub(crate) const FILE_URL_COMMIT: &str = "file:";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Manifest {
    skills: HashMap<String, SkillEntry>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SkillEntry {
    pub(crate) source: String,
    pub(crate) path: String,
    pub(crate) updated: String,
    pub(crate) version: String,
    pub(crate) description: String,
    pub(crate) commit: String,
    pub(crate) branch: String,
    pub(crate) remote: String,
}

impl SkillEntry {
    pub fn new(
        source: &str,
        path: &str,
        frontmatter: &SkillFrontmatter,
        commit: &str,
        remote: &str,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            source: source.to_string(),
            path: path.to_string(),
            updated: now,
            version: frontmatter.version.clone().unwrap_or_default(),
            description: frontmatter.description.clone(),
            commit: commit.to_string(),
            branch: crate::DEFAULT_BRANCH.to_string(),
            remote: remote.to_string(),
        }
    }

    pub fn update(&mut self, commit: &str, version: &str, branch: &str, description: &str) {
        self.commit = commit.to_string();
        self.version = version.to_string();
        self.branch = branch.to_string();
        self.description = description.to_string();
        self.updated = chrono::Utc::now().to_rfc3339();
    }

    pub fn set_branch(&mut self, branch: &str) {
        self.branch = branch.to_string();
    }

    /// The source used to (re)install this skill: the remote URL when the
    /// remote starts with the FILE_URL_COMMIT marker (a `file://` install),
    /// the tracked source otherwise.
    pub fn install_source(&self) -> String {
        if self.remote.starts_with(FILE_URL_COMMIT) {
            self.remote.clone()
        } else {
            self.source.clone()
        }
    }

    /// True when this skill was installed from a file:// SKILL.md URL,
    /// identified by the FILE_URL_COMMIT marker in the commit field.
    /// (file:// URLs may also host plain git repos; their entries carry a
    /// real commit hash and are not file skills.)
    pub fn is_file_url(&self) -> bool {
        self.commit.starts_with(FILE_URL_COMMIT)
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
}

impl Manifest {
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(path).context("Failed to read manifest")?;
        let manifest: Manifest =
            serde_json::from_str(&content).context("Failed to parse manifest")?;
        for name in manifest.skills.keys() {
            crate::core::spec::validate_skill_name(name)
                .with_context(|| format!("Invalid skill name in manifest: {name}"))?;
        }
        Ok(manifest)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self).context("Failed to serialize manifest")?;
        fs::write(path, content).context("Failed to write manifest")?;
        Ok(())
    }

    pub fn add_skill(&mut self, name: &str, entry: SkillEntry) {
        self.skills.insert(name.to_string(), entry);
    }

    pub fn remove_skill(&mut self, name: &str) -> Option<SkillEntry> {
        self.skills.remove(name)
    }

    pub fn is_tracked(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    pub fn skill_names(&self) -> Vec<String> {
        self.skills.keys().cloned().collect()
    }

    pub fn get_skill(&self, name: &str) -> Option<&SkillEntry> {
        self.skills.get(name)
    }

    pub fn all_skills(&self) -> Vec<(&str, &SkillEntry)> {
        self.skills.iter().map(|(k, v)| (k.as_str(), v)).collect()
    }

    /// Resolve a user-provided identifier to manifest skill names.
    ///
    /// Accepts a raw skill name (direct key match), an `owner/repo` label,
    /// or a full URL. Returns all matching skill names (multiple for multi-skill repos).
    pub fn resolve_names(&self, input: &str) -> Vec<String> {
        if self.is_tracked(input) {
            return vec![input.to_string()];
        }

        let label = if crate::core::parse_source::is_url(input) {
            crate::core::parse_source::repo_label(input).to_string()
        } else {
            input.to_string()
        };

        self.skills
            .iter()
            .filter(|(_, entry)| {
                entry.source == label || entry.remote == input || entry.install_source() == input
            })
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub fn update_skill(
        &mut self,
        name: &str,
        commit: &str,
        version: &str,
        branch: &str,
        description: String,
    ) {
        if let Some(entry) = self.skills.get_mut(name) {
            entry.update(commit, version, branch, &description);
        }
    }

    pub fn rebuild_from_directory(skills_dir: &Path) -> Result<Self> {
        let mut manifest = Self::new();

        for skill in crate::core::scan::scan_skills_dir(skills_dir)? {
            crate::core::spec::validate_skill_name(&skill.frontmatter.name)?;
            crate::core::spec::validate_skill_name(&skill.dir_name)?;

            manifest.add_skill(
                &skill.dir_name,
                SkillEntry::new("unknown", "", &skill.frontmatter, "unknown", "unknown"),
            );
        }

        Ok(manifest)
    }

    pub fn find_stale(&self, skills_dir: &Path) -> Vec<String> {
        let mut stale = Vec::new();

        for name in self.skills.keys() {
            let skill_dir = skills_dir.join(name);
            if !skill_dir.exists() {
                stale.push(name.clone());
            }
        }

        stale
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_load_empty_manifest() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join(".skills.json");
        let manifest = Manifest::load(&path).unwrap();
        assert!(manifest.is_empty());
    }

    #[test]
    fn test_load_rejects_unsafe_skill_name() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join(".skills.json");
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "../escape",
            SkillEntry {
                source: "owner/repo".into(),
                path: "".into(),
                updated: "u".into(),
                version: "v".into(),
                description: "d".into(),
                commit: "c".into(),
                branch: "main".into(),
                remote: "remote".into(),
            },
        );
        manifest.save(&path).unwrap();

        assert!(Manifest::load(&path).is_err());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join(".skills.json");

        let mut manifest = Manifest::new();
        let entry = SkillEntry {
            source: "test/repo".into(),
            path: "skills/test-skill".into(),
            updated: "2026-01-01T00:00:00Z".into(),
            version: "1.0.0".into(),
            description: "Test skill".into(),
            commit: "abc1234".into(),
            branch: "main".into(),
            remote: "https://github.com/test/repo.git".into(),
        };
        manifest.add_skill("test-skill", entry);

        manifest.save(&path).unwrap();
        assert!(path.exists());

        let loaded = Manifest::load(&path).unwrap();
        assert_eq!(loaded.skill_names().len(), 1);
        let loaded_entry = loaded.get_skill("test-skill").unwrap();
        assert_eq!(loaded_entry.source, "test/repo");
        assert_eq!(loaded_entry.commit, "abc1234");
    }

    #[test]
    fn test_is_tracked() {
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "tracked",
            SkillEntry {
                source: "s".into(),
                path: "p".into(),
                updated: "u".into(),
                version: "v".into(),
                description: "d".into(),
                commit: "c".into(),
                branch: "b".into(),
                remote: "r".into(),
            },
        );
        assert!(manifest.is_tracked("tracked"));
        assert!(!manifest.is_tracked("untracked"));
    }

    #[test]
    fn test_remove_skill() {
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "to-remove",
            SkillEntry {
                source: "s".into(),
                path: "p".into(),
                updated: "u".into(),
                version: "v".into(),
                description: "d".into(),
                commit: "c".into(),
                branch: "b".into(),
                remote: "r".into(),
            },
        );
        let removed = manifest.remove_skill("to-remove");
        assert!(removed.is_some());
        assert!(manifest.is_empty());
    }

    #[test]
    fn test_find_stale_detects_missing_dirs() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        // Create one real skill
        let real_skill = skills_dir.join("real-skill");
        fs::create_dir_all(&real_skill).unwrap();
        let real_md = real_skill.join("SKILL.md");
        fs::write(&real_md, "---\nname: Real Skill\ndescription: Real\n---\n").unwrap();

        // Manifest references two skills: "real-skill" and "ghost-skill"
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "real-skill",
            SkillEntry {
                source: "s".into(),
                path: "p".into(),
                updated: "u".into(),
                version: "v".into(),
                description: "d".into(),
                commit: "c".into(),
                branch: "b".into(),
                remote: "r".into(),
            },
        );
        manifest.add_skill(
            "ghost-skill",
            SkillEntry {
                source: "s".into(),
                path: "p".into(),
                updated: "u".into(),
                version: "v".into(),
                description: "d".into(),
                commit: "c".into(),
                branch: "b".into(),
                remote: "r".into(),
            },
        );

        let removed = manifest.find_stale(&skills_dir);
        assert_eq!(removed, vec!["ghost-skill"]);
        assert!(manifest.is_tracked("real-skill"));
        assert!(
            manifest.is_tracked("ghost-skill"),
            "Stale entry preserved to avoid data loss"
        );
    }

    #[test]
    fn test_find_stale_preserves_entries() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Manifest references a skill whose directory doesn't exist
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "orphan",
            SkillEntry {
                source: "owner/repo".into(),
                path: "skills/orphan".into(),
                updated: "u".into(),
                version: "1.0.0".into(),
                description: "Important skill".into(),
                commit: "abc123".into(),
                branch: "main".into(),
                remote: "https://github.com/owner/repo".into(),
            },
        );

        let removed = manifest.find_stale(&skills_dir);
        // Should report the stale entry but NOT remove it
        assert_eq!(removed, vec!["orphan"]);
        assert!(
            manifest.is_tracked("orphan"),
            "Stale entry should be preserved to avoid data loss"
        );
    }

    #[test]
    fn test_rebuild_from_directory() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");

        for name in &["skill-a", "skill-b"] {
            let dir = skills_dir.join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("SKILL.md"),
                format!(
                    "---\nname: {}\ndescription: Some desc\n---\n# {}\n",
                    name, name
                ),
            )
            .unwrap();
        }

        // Add a non-skill directory
        fs::create_dir_all(skills_dir.join(".hidden")).unwrap();
        fs::create_dir_all(skills_dir.join("not-a-skill")).unwrap();

        let manifest = Manifest::rebuild_from_directory(&skills_dir).unwrap();
        assert_eq!(manifest.skill_names().len(), 2);
        assert!(manifest.is_tracked("skill-a"));
        assert!(manifest.is_tracked("skill-b"));
        assert!(!manifest.is_tracked("not-a-skill"));

        let entry = manifest.get_skill("skill-a").unwrap();
        assert_eq!(entry.source, "unknown");
        assert_eq!(entry.description, "Some desc");
    }

    #[test]
    fn test_rebuild_empty_directory() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("empty");
        fs::create_dir(&dir).unwrap();
        let manifest = Manifest::rebuild_from_directory(&dir).unwrap();
        assert!(manifest.is_empty());
    }

    #[test]
    fn test_skill_entry_new() {
        let fm = SkillFrontmatter {
            name: "Test".into(),
            description: "Desc".into(),
            version: Some("2.0.0".into()),
        };
        let entry = SkillEntry::new(
            "test/repo",
            "skills/test",
            &fm,
            "abcdef7",
            "https://github.com/test/repo.git",
        );
        assert_eq!(entry.source, "test/repo");
        assert_eq!(entry.path, "skills/test");
        assert_eq!(entry.version, "2.0.0");
        assert_eq!(entry.commit, "abcdef7");
        assert_eq!(entry.branch, "main");
        assert_eq!(entry.remote, "https://github.com/test/repo.git");
        // updated should be a recent timestamp
        assert!(!entry.updated.is_empty());
    }

    #[test]
    fn test_install_source_git_repo() {
        let entry = SkillEntry {
            source: "owner/repo".into(),
            path: "skills/test".into(),
            updated: "u".into(),
            version: "v".into(),
            description: "d".into(),
            commit: "c".into(),
            branch: "main".into(),
            remote: "https://github.com/owner/repo.git".into(),
        };
        assert_eq!(entry.install_source(), "owner/repo");
    }

    #[test]
    fn test_install_source_file_url() {
        let entry = SkillEntry {
            source: "".into(),
            path: "skills/test".into(),
            updated: "u".into(),
            version: "v".into(),
            description: "d".into(),
            commit: "c".into(),
            branch: "main".into(),
            remote: "file:///Users/test/SKILL.md".into(),
        };
        assert_eq!(entry.install_source(), "file:///Users/test/SKILL.md");
    }

    #[test]
    fn test_install_source_empty_source_uses_remote() {
        let entry = SkillEntry {
            source: "".into(),
            path: "skills/test".into(),
            updated: "u".into(),
            version: "v".into(),
            description: "d".into(),
            commit: "c".into(),
            branch: "main".into(),
            remote: "https://github.com/owner/repo".into(),
        };
        assert_eq!(entry.install_source(), "");
    }

    #[test]
    fn test_skill_entry_new_no_version() {
        let fm = SkillFrontmatter {
            name: "NoVersion".into(),
            description: "Skill without version field".into(),
            version: None,
        };
        let entry = SkillEntry::new("test/repo", "skills/test", &fm, "abcdef7", "url");
        assert!(
            entry.version.is_empty(),
            "Version should be empty when frontmatter has no version, got: {:?}",
            entry.version
        );
        assert_eq!(entry.description, "Skill without version field");
    }

    #[test]
    fn test_skill_entry_update() {
        let fm = SkillFrontmatter {
            name: "Test".into(),
            description: "Desc".into(),
            version: Some("1.0.0".into()),
        };
        let mut entry = SkillEntry::new("r", "p", &fm, "oldhash", "url");
        let old_updated = entry.updated.clone();

        // Sleep to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        entry.update("newhash", "2.0.0", "dev", "New description");
        assert_eq!(entry.commit, "newhash");
        assert_eq!(entry.version, "2.0.0");
        assert_eq!(entry.branch, "dev");
        assert_eq!(entry.description, "New description");
        assert!(entry.updated > old_updated);
    }

    fn make_manifest_with_source(source: &str, name: &str) -> Manifest {
        let mut manifest = Manifest::new();
        manifest.add_skill(
            name,
            SkillEntry {
                source: source.into(),
                path: "p".into(),
                updated: "u".into(),
                version: "v".into(),
                description: "d".into(),
                commit: "c".into(),
                branch: "b".into(),
                remote: "https://github.com/".to_string() + source + ".git",
            },
        );
        manifest
    }

    #[test]
    fn test_resolve_names_direct_match() {
        let manifest = make_manifest_with_source("owner/repo", "my-skill");
        assert_eq!(manifest.resolve_names("my-skill"), vec!["my-skill"]);
    }

    #[test]
    fn test_resolve_names_by_source_label() {
        let manifest = make_manifest_with_source("leonardomso/rust-skills", "git-commit");
        assert_eq!(
            manifest.resolve_names("leonardomso/rust-skills"),
            vec!["git-commit"]
        );
    }

    #[test]
    fn test_resolve_names_by_url() {
        let manifest = make_manifest_with_source("leonardomso/rust-skills", "git-commit");
        assert_eq!(
            manifest.resolve_names("https://github.com/leonardomso/rust-skills"),
            vec!["git-commit"]
        );
    }

    #[test]
    fn test_resolve_names_by_file_url() {
        let mut manifest = Manifest::new();
        manifest.add_skill(
            "file-skill",
            SkillEntry {
                source: "example.com".into(),
                path: "".into(),
                updated: "u".into(),
                version: "v".into(),
                description: "d".into(),
                commit: FILE_URL_COMMIT.into(),
                branch: "".into(),
                remote: "file:///tmp/SKILL.md".into(),
            },
        );

        assert_eq!(
            manifest.resolve_names("file:///tmp/SKILL.md"),
            vec!["file-skill"]
        );
    }

    #[test]
    fn test_resolve_names_multi_skill_repo() {
        let mut manifest = Manifest::new();
        for name in &["git-commit", "code-review"] {
            manifest.add_skill(
                name,
                SkillEntry {
                    source: "leonardomso/rust-skills".into(),
                    path: "".into(),
                    updated: "u".into(),
                    version: "v".into(),
                    description: "d".into(),
                    commit: "c".into(),
                    branch: "b".into(),
                    remote: "https://github.com/leonardomso/rust-skills.git".into(),
                },
            );
        }
        let mut names = manifest.resolve_names("leonardomso/rust-skills");
        names.sort();
        assert_eq!(names, vec!["code-review", "git-commit"]);
    }

    #[test]
    fn test_resolve_names_no_match() {
        let manifest = make_manifest_with_source("owner/repo", "my-skill");
        assert!(manifest.resolve_names("unknown-skill").is_empty());
        assert!(manifest.resolve_names("other/repo").is_empty());
    }
}
