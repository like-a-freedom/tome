use anyhow::{Context, Result};
use crossterm::terminal;
use serde::Serialize;
use std::fs;
use std::path::Path;

use crate::core::manifest::Manifest;
use crate::core::spec;

pub fn execute(
    json: bool,
    skills_dir: &Path,
    manifest_path: &Path,
) -> Result<()> {
    // 1. Load or rebuild manifest
    let mut manifest = if !manifest_path.exists() {
        let rebuilt = Manifest::rebuild_from_directory(skills_dir)?;
        if json {
            // Continue silently — manifest will be saved at the end if skills exist
        } else if rebuilt.skills.is_empty() {
            println!("No skills installed.");
            return Ok(());
        } else {
            eprintln!(
                "No manifest found. Rebuilt from {} skill(s) in directory.",
                rebuilt.skills.len()
            );
        }
        rebuilt
    } else {
        Manifest::load(manifest_path)?
    };

    // 2. Clean stale entries
    let stale = manifest.clean_stale(skills_dir);
    if !stale.is_empty() {
        eprintln!(
            "Cleaned up {} stale manifest entries: {}",
            stale.len(),
            stale.join(", ")
        );
    }

    // 3. Scan for untracked skills
    let untracked = scan_untracked_skills(skills_dir, &manifest)?;

    // 4. Output
    if json {
        output_json(&manifest, &untracked)?;
    } else {
        output_table(&manifest, &untracked);
    }

    // 5. Save manifest (in case it was rebuilt or stale entries removed)
    if !manifest_path.exists() || !stale.is_empty() {
        manifest.save(manifest_path)?;
    }

    Ok(())
}

fn scan_untracked_skills(
    skills_dir: &Path,
    manifest: &Manifest,
) -> Result<Vec<UntrackedSkill>> {
    let mut untracked = Vec::new();

    if !skills_dir.exists() {
        return Ok(untracked);
    }

    for entry in fs::read_dir(skills_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }

        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }

        let dir_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // Skip if already in manifest
        if manifest.is_tracked(dir_name) {
            continue;
        }

        // Try to parse frontmatter for display
        if let Ok(content) = fs::read_to_string(&skill_md) {
            if let Ok(fm) = spec::parse_frontmatter(&content) {
                // Check if tracked by frontmatter name too
                if manifest.is_tracked(&fm.name) {
                    continue;
                }
                untracked.push(UntrackedSkill {
                    name: fm.name,
                    dir_name: dir_name.to_string(),
                    description: fm.description,
                    version: fm.version.unwrap_or("0.0.0".into()),
                });
            }
        }
    }

    Ok(untracked)
}

fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        s.to_string()
    } else if max_width <= 3 {
        s[..max_width].to_string()
    } else {
        format!("{}...", &s[..max_width - 3])
    }
}

fn output_table(manifest: &Manifest, untracked: &[UntrackedSkill]) {
    if manifest.skills.is_empty() && untracked.is_empty() {
        println!("No skills installed.");
        return;
    }

    let (cols, _) = terminal::size().unwrap_or((100, 24));
    let term_width = cols as usize;

    let mut max_name = 4usize;
    let mut max_version = 7usize;
    let mut max_source = 6usize;

    for (name, entry) in &manifest.skills {
        max_name = max_name.max(name.len());
        max_version = max_version.max(entry.version.len());
        max_source = max_source.max(entry.source.len());
    }
    for skill in untracked {
        max_name = max_name.max(skill.name.len());
        max_version = max_version.max(skill.version.len());
        max_source = max_source.max(1);
    }

    let tracked_w = 7usize;
    let gaps = 3usize;

    let mut name_w = max_name.min(30);
    let version_w = max_version.min(15);
    let mut source_w = max_source.min(40);

    let total = name_w + version_w + source_w + tracked_w + gaps;
    if total > term_width.saturating_sub(1) {
        let overflow = total - term_width.saturating_sub(1);
        let source_slack = source_w.saturating_sub(6);
        let name_slack = name_w.saturating_sub(8);
        let cut_source = overflow.min(source_slack);
        let remaining = overflow.saturating_sub(cut_source);
        let cut_name = remaining.min(name_slack);
        source_w -= cut_source;
        name_w -= cut_name;
    } else {
        let available = term_width.saturating_sub(1) - total;
        source_w += available.min(max_source.saturating_sub(source_w));
    }

    println!(
        "{:<name_w$} {:<version_w$} {:<source_w$} {:<tracked_w$}",
        "NAME", "VERSION", "SOURCE", "TRACKED",
        name_w = name_w,
        version_w = version_w,
        source_w = source_w,
        tracked_w = tracked_w,
    );
    let sep_len = name_w + version_w + source_w + tracked_w + gaps;
    println!("{}", "-".repeat(sep_len));

    for (name, entry) in &manifest.skills {
        println!(
            "{:<name_w$} {:<version_w$} {:<source_w$} {:<tracked_w$}",
            truncate_with_ellipsis(name, name_w),
            truncate_with_ellipsis(&entry.version, version_w),
            truncate_with_ellipsis(&entry.source, source_w),
            "yes",
            name_w = name_w,
            version_w = version_w,
            source_w = source_w,
            tracked_w = tracked_w,
        );
    }

    for skill in untracked {
        println!(
            "{:<name_w$} {:<version_w$} {:<source_w$} {:<tracked_w$}",
            truncate_with_ellipsis(&skill.name, name_w),
            truncate_with_ellipsis(&skill.version, version_w),
            truncate_with_ellipsis("?", source_w),
            "no",
            name_w = name_w,
            version_w = version_w,
            source_w = source_w,
            tracked_w = tracked_w,
        );
    }
}

fn output_json(manifest: &Manifest, untracked: &[UntrackedSkill]) -> Result<()> {
    #[derive(Serialize)]
    struct JsonSkill {
        name: String,
        version: String,
        source: String,
        tracked: bool,
        description: String,
    }

    let mut skills: Vec<JsonSkill> = Vec::new();

    for (name, entry) in &manifest.skills {
        skills.push(JsonSkill {
            name: name.clone(),
            version: entry.version.clone(),
            source: entry.source.clone(),
            tracked: true,
            description: entry.description.clone(),
        });
    }

    for skill in untracked {
        skills.push(JsonSkill {
            name: skill.name.clone(),
            version: skill.version.clone(),
            source: "?".to_string(),
            tracked: false,
            description: skill.description.clone(),
        });
    }

    let output = serde_json::to_string(&skills)
        .context("Failed to serialize output")?;
    println!("{}", output);
    Ok(())
}

struct UntrackedSkill {
    name: String,
    #[allow(dead_code)]
    dir_name: String,
    description: String,
    version: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::SkillEntry;
    use crate::core::spec::SkillFrontmatter;
    use std::fs;
    use tempfile::TempDir;

    fn make_fm(name: &str) -> SkillFrontmatter {
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

    #[test]
    fn test_list_empty_skills_dir() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_rebuild_table_output() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("Orphan")).unwrap();
        fs::write(skills_dir.join("Orphan").join("SKILL.md"), "---\nname: Orphan\ndescription: no manifest\n---\n").unwrap();
        // No .skills.json
        let manifest_path = skills_dir.join(".skills.json");

        let result = execute(false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
        assert!(manifest_path.exists());
    }

    #[test]
    fn test_list_table_with_tracked_and_untracked() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("Tracked")).unwrap();
        fs::write(skills_dir.join("Tracked").join("SKILL.md"), "---\nname: Tracked\ndescription: t\n---\n").unwrap();
        fs::create_dir_all(skills_dir.join("Untracked")).unwrap();
        fs::write(skills_dir.join("Untracked").join("SKILL.md"), "---\nname: Untracked\ndescription: u\n---\n").unwrap();

        let manifest_path = skills_dir.join(".skills.json");
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        manifest.add_skill("Tracked", SkillEntry::new("test/repo", "", &make_fm("Tracked"), "abc", "url"));
        manifest.save(&manifest_path).unwrap();

        let result = execute(false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_nonexistent_skills_dir() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("nonexistent");
        let manifest_path = temp.path().join(".skills.json");
        fs::write(&manifest_path, r#"{"version":1,"skills":{}}"#).unwrap();

        let result = execute(false, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_list_stale_cleanup() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        // Manifest references a skill that doesn't exist on disk
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        manifest.add_skill("Ghost", SkillEntry::new("test/repo", "", &make_fm("Ghost"), "abc", "url"));
        manifest.save(&manifest_path).unwrap();

        let result = execute(false, &skills_dir, &manifest_path);
        assert!(result.is_ok());

        // Verify ghost was cleaned
        let loaded = Manifest::load(&manifest_path).unwrap();
        assert!(!loaded.skills.contains_key("Ghost"));
    }

    #[test]
    fn test_scan_untracked_with_empty_dir() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };

        let untracked = scan_untracked_skills(&skills_dir, &manifest).unwrap();
        assert!(untracked.is_empty());
    }

    #[test]
    fn test_scan_untracked_skips_dot_dirs() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join(".hidden")).unwrap();
        fs::write(skills_dir.join(".hidden").join("SKILL.md"), "---\nname: Hidden\ndescription: d\n---\n").unwrap();
        let manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };

        let untracked = scan_untracked_skills(&skills_dir, &manifest).unwrap();
        assert!(untracked.is_empty());
    }

    #[test]
    fn test_scan_untracked_skips_dirs_without_skill_md() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("no-skill")).unwrap();
        fs::write(skills_dir.join("no-skill").join("README.md"), "not a skill").unwrap();
        let manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };

        let untracked = scan_untracked_skills(&skills_dir, &manifest).unwrap();
        assert!(untracked.is_empty());
    }

    #[test]
    fn test_scan_untracked_tracks_by_frontmatter_name() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("dir-name")).unwrap();
        fs::write(skills_dir.join("dir-name").join("SKILL.md"), "---\nname: FmName\ndescription: d\n---\n").unwrap();
        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        // Track by frontmatter name (not dir name)
        manifest.add_skill("FmName", SkillEntry::new("test/repo", "", &make_fm("FmName"), "abc", "url"));

        let untracked = scan_untracked_skills(&skills_dir, &manifest).unwrap();
        assert!(untracked.is_empty());
    }

    #[test]
    fn test_output_json_with_both_types() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let mut manifest = Manifest { version: 1, skills: std::collections::HashMap::new() };
        manifest.add_skill("Tracked", SkillEntry::new("test/repo", "", &make_fm("Tracked"), "abc", "url"));
        manifest.save(&manifest_path).unwrap();

        fs::create_dir_all(skills_dir.join("Manual")).unwrap();
        fs::write(skills_dir.join("Manual").join("SKILL.md"), "---\nname: Manual\ndescription: m\n---\n").unwrap();

        let result = execute(true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }
}
