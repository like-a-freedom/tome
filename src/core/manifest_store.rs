use crate::core::manifest::{Manifest, SkillEntry};
use anyhow::Result;
use std::path::Path;

pub trait ManifestStore {
    fn is_tracked(&self, name: &str) -> bool;
    fn get_skill(&self, name: &str) -> Option<&SkillEntry>;
    fn all_skills(&self) -> Vec<(&str, &SkillEntry)>;
    fn skill_names(&self) -> Vec<String>;
    fn add_skill(&mut self, name: &str, entry: SkillEntry);
    fn remove_skill(&mut self, name: &str) -> Option<SkillEntry>;
    fn update_skill(
        &mut self,
        name: &str,
        commit: &str,
        version: &str,
        branch: &str,
        description: String,
    );
    fn is_empty(&self) -> bool;
    fn resolve_names(&self, input: &str) -> Vec<String>;
    fn find_stale(&self, skills_dir: &Path) -> Vec<String>;
    fn save(&self, path: &Path) -> Result<()>;
}

impl ManifestStore for Manifest {
    fn is_tracked(&self, name: &str) -> bool {
        Manifest::is_tracked(self, name)
    }

    fn get_skill(&self, name: &str) -> Option<&SkillEntry> {
        Manifest::get_skill(self, name)
    }

    fn all_skills(&self) -> Vec<(&str, &SkillEntry)> {
        self.skills_map()
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }

    fn skill_names(&self) -> Vec<String> {
        Manifest::skill_names(self)
    }

    fn add_skill(&mut self, name: &str, entry: SkillEntry) {
        Manifest::add_skill(self, name, entry);
    }

    fn remove_skill(&mut self, name: &str) -> Option<SkillEntry> {
        Manifest::remove_skill(self, name)
    }

    fn update_skill(
        &mut self,
        name: &str,
        commit: &str,
        version: &str,
        branch: &str,
        description: String,
    ) {
        Manifest::update_skill(self, name, commit, version, branch, description);
    }

    fn is_empty(&self) -> bool {
        Manifest::is_empty(self)
    }

    fn resolve_names(&self, input: &str) -> Vec<String> {
        Manifest::resolve_names(self, input)
    }

    fn find_stale(&self, skills_dir: &Path) -> Vec<String> {
        Manifest::find_stale(self, skills_dir)
    }

    fn save(&self, path: &Path) -> Result<()> {
        Manifest::save(self, path)
    }
}

pub struct MemoryManifestStore {
    skills: std::collections::HashMap<String, SkillEntry>,
}

impl Default for MemoryManifestStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryManifestStore {
    pub fn new() -> Self {
        Self {
            skills: std::collections::HashMap::new(),
        }
    }
}

impl ManifestStore for MemoryManifestStore {
    fn is_tracked(&self, name: &str) -> bool {
        self.skills.contains_key(name)
    }

    fn get_skill(&self, name: &str) -> Option<&SkillEntry> {
        self.skills.get(name)
    }

    fn all_skills(&self) -> Vec<(&str, &SkillEntry)> {
        self.skills.iter().map(|(k, v)| (k.as_str(), v)).collect()
    }

    fn skill_names(&self) -> Vec<String> {
        self.skills.keys().cloned().collect()
    }

    fn add_skill(&mut self, name: &str, entry: SkillEntry) {
        self.skills.insert(name.to_string(), entry);
    }

    fn remove_skill(&mut self, name: &str) -> Option<SkillEntry> {
        self.skills.remove(name)
    }

    fn update_skill(
        &mut self,
        name: &str,
        commit: &str,
        version: &str,
        branch: &str,
        description: String,
    ) {
        if let Some(entry) = self.skills.get_mut(name) {
            entry.update(commit, version);
            entry.branch = branch.to_string();
            entry.description = description;
        }
    }

    fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    fn resolve_names(&self, input: &str) -> Vec<String> {
        if self.is_tracked(input) {
            return vec![input.to_string()];
        }

        let label = if input.starts_with("http://")
            || input.starts_with("https://")
            || input.starts_with("git@")
            || input.starts_with("ssh://")
        {
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

    fn find_stale(&self, skills_dir: &Path) -> Vec<String> {
        let mut stale = Vec::new();

        for name in self.skills.keys() {
            let skill_dir = skills_dir.join(name);
            if !skill_dir.exists() {
                stale.push(name.clone());
            }
        }

        stale
    }

    fn save(&self, _path: &Path) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::spec::SkillFrontmatter;
    use tempfile::TempDir;

    fn make_skill_entry(name: &str) -> SkillEntry {
        let fm = SkillFrontmatter {
            name: name.to_string(),
            description: format!("desc for {name}"),
            version: Some("1.0.0".into()),
            authors: None,
            tags: None,
            platforms: None,
            license: None,
            repository: None,
            docs: None,
        };
        SkillEntry::new(
            "owner/repo",
            "",
            &fm,
            "abc123",
            "https://github.com/owner/repo.git",
        )
    }

    #[test]
    fn memory_store_crud() {
        let mut store = MemoryManifestStore::new();
        assert!(store.is_empty());

        let entry = make_skill_entry("test-skill");
        store.add_skill("test-skill", entry);
        assert!(!store.is_empty());
        assert!(store.is_tracked("test-skill"));
        assert_eq!(store.skill_names(), vec!["test-skill".to_string()]);

        let skills = store.all_skills();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].0, "test-skill");

        let removed = store.remove_skill("test-skill");
        assert!(removed.is_some());
        assert!(store.is_empty());
    }

    #[test]
    fn memory_store_update_skill() {
        let mut store = MemoryManifestStore::new();
        let entry = make_skill_entry("skill");
        store.add_skill("skill", entry);

        store.update_skill("skill", "newhash", "2.0.0", "dev", "updated".into());

        let entry = store.get_skill("skill").unwrap();
        assert_eq!(entry.commit, "newhash");
        assert_eq!(entry.version, "2.0.0");
        assert_eq!(entry.branch, "dev");
        assert_eq!(entry.description, "updated");
    }

    #[test]
    fn memory_store_find_stale() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        std::fs::create_dir_all(skills_dir.join("real")).unwrap();

        let mut store = MemoryManifestStore::new();
        store.add_skill("real", make_skill_entry("real"));
        store.add_skill("ghost", make_skill_entry("ghost"));

        let stale = store.find_stale(&skills_dir);
        assert_eq!(stale, vec!["ghost".to_string()]);
    }

    #[test]
    fn memory_store_resolve_names() {
        let mut store = MemoryManifestStore::new();
        let mut entry = make_skill_entry("skill");
        entry.source = "owner/repo".to_string();
        entry.remote = "https://github.com/owner/repo.git".to_string();
        store.add_skill("skill", entry);

        assert_eq!(store.resolve_names("skill"), vec!["skill".to_string()]);
        assert_eq!(store.resolve_names("owner/repo"), vec!["skill".to_string()]);
        assert!(store.resolve_names("unknown").is_empty());
    }

    #[test]
    fn memory_store_save_is_noop() {
        let store = MemoryManifestStore::new();
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("test.json");
        assert!(store.save(&path).is_ok());
    }
}
