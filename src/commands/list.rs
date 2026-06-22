use anyhow::{Context, Result};
use crossterm::terminal;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::core::manifest::Manifest;
use crate::core::parse_source;
use crate::core::spec;
use crate::style::{self, BOLD, CYAN, DIM};

pub fn execute(json: bool, skills_dir: &Path, manifest_path: &Path) -> Result<()> {
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

fn scan_untracked_skills(skills_dir: &Path, manifest: &Manifest) -> Result<Vec<UntrackedSkill>> {
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

        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.starts_with('.')
        {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }

        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // Skip if already in manifest
        if manifest.is_tracked(dir_name) {
            continue;
        }

        // Try to parse frontmatter for display
        if let Ok(content) = fs::read_to_string(&skill_md)
            && let Ok(fm) = spec::parse_frontmatter(&content)
        {
            // Check if tracked by frontmatter name too
            if manifest.is_tracked(&fm.name) {
                continue;
            }
            untracked.push(UntrackedSkill {
                name: fm.name,
                dir_name: dir_name.to_string(),
                description: fm.description,
                version: fm.version.unwrap_or(NO_VERSION_SENTINEL.into()),
            });
        }
    }

    Ok(untracked)
}

fn truncate_with_ellipsis(s: &str, max_width: usize) -> String {
    let ellipsis_len = 3;
    if s.len() <= max_width {
        s.to_string()
    } else if max_width <= ellipsis_len {
        s[..max_width].to_string()
    } else {
        format!("{}...", &s[..max_width - ellipsis_len])
    }
}

fn display_version(v: &str) -> &str {
    if v.is_empty() || v == NO_VERSION_SENTINEL {
        "-"
    } else {
        v
    }
}

struct TableWidths {
    name: usize,
    version: usize,
    tracked: usize,
}

impl TableWidths {
    const TRACKED_WIDTH: usize = 7;
    const GAPS: usize = 2;
    const MIN_NAME_WIDTH: usize = 6;
    const MIN_VERSION_WIDTH: usize = 9;
    const MAX_NAME_WIDTH: usize = 80;
    const MAX_VERSION_WIDTH: usize = 15;
    const MIN_SLACK_NAME: usize = 16;

    fn calculate(manifest: &Manifest, untracked: &[UntrackedSkill], term_width: usize) -> Self {
        let mut max_name = Self::MIN_NAME_WIDTH;
        let mut max_version = Self::MIN_VERSION_WIDTH;

        for (name, entry) in &manifest.skills {
            max_name = max_name.max(name.len());
            max_version = max_version.max(display_version(&entry.version).len());
        }
        for skill in untracked {
            max_name = max_name.max(skill.name.len());
            max_version = max_version.max(display_version(&skill.version).len());
        }

        let mut name_w = max_name.min(Self::MAX_NAME_WIDTH);
        let version_w = max_version.min(Self::MAX_VERSION_WIDTH);

        let total = name_w + version_w + Self::TRACKED_WIDTH + Self::GAPS;
        if total > term_width.saturating_sub(1) {
            let overflow = total - term_width.saturating_sub(1);
            let name_slack = name_w.saturating_sub(Self::MIN_SLACK_NAME);
            let cut_name = overflow.min(name_slack);
            name_w -= cut_name;
        }

        TableWidths {
            name: name_w,
            version: version_w,
            tracked: Self::TRACKED_WIDTH,
        }
    }

    fn total(&self) -> usize {
        self.name + self.version + self.tracked + Self::GAPS
    }
}

fn output_table(manifest: &Manifest, untracked: &[UntrackedSkill]) {
    if manifest.is_empty() && untracked.is_empty() {
        println!("No skills installed.");
        return;
    }

    let (cols, _) = terminal::size().unwrap_or((DEFAULT_COLS, DEFAULT_ROWS));
    let w = TableWidths::calculate(manifest, untracked, cols as usize);

    // Build rows and group by source
    let mut groups: BTreeMap<String, Vec<(String, String, bool)>> = BTreeMap::new();

    for (name, entry) in &manifest.skills {
        let label = parse_source::repo_label(&entry.source);
        groups.entry(label.to_string()).or_default().push((
            name.clone(),
            entry.version.clone(),
            true,
        ));
    }

    for skill in untracked {
        groups.entry(String::new()).or_default().push((
            skill.name.clone(),
            skill.version.clone(),
            false,
        ));
    }

    // Sort skills within each group by name
    for rows in groups.values_mut() {
        rows.sort_by(|a, b| a.0.cmp(&b.0));
    }

    // Print header
    println!(
        "{:<name_w$} {:<version_w$} {:<tracked_w$}",
        "NAME",
        "VERSION",
        "TRACKED",
        name_w = w.name,
        version_w = w.version,
        tracked_w = w.tracked,
    );
    println!("{}", "-".repeat(w.total()));

    // Print tracked groups (non-empty key = has source)
    let mut first_tracked = true;
    for (source, rows) in &groups {
        if source.is_empty() {
            continue;
        }
        if first_tracked {
            first_tracked = false;
        } else {
            println!();
        }
        println!("{}{}── {} ──{}", BOLD, CYAN, source, style::RESET,);
        for (name, version, tracked) in rows {
            let tracked_str = if *tracked { "yes" } else { "no" };
            println!(
                "{:<name_w$} {:<version_w$} {:<tracked_w$}",
                truncate_with_ellipsis(name, w.name),
                truncate_with_ellipsis(display_version(version), w.version),
                tracked_str,
                name_w = w.name,
                version_w = w.version,
                tracked_w = w.tracked,
            );
        }
    }

    // Print untracked group (empty key)
    if let Some(rows) = groups.get(&String::new())
        && !rows.is_empty()
    {
        println!();
        println!("{}", style::styled(DIM, "── no repository ──"),);
        for (name, version, _) in rows {
            println!(
                "{:<name_w$} {:<version_w$} {:<tracked_w$}",
                truncate_with_ellipsis(name, w.name),
                truncate_with_ellipsis(display_version(version), w.version),
                "no",
                name_w = w.name,
                version_w = w.version,
                tracked_w = w.tracked,
            );
        }
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

    let output = serde_json::to_string(&skills).context("Failed to serialize output")?;
    println!("{}", output);
    Ok(())
}

const NO_VERSION_SENTINEL: &str = "0.0.0";
const DEFAULT_COLS: u16 = 100;
const DEFAULT_ROWS: u16 = 24;

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
    use crate::testutil;
    use std::fs;
    use tempfile::TempDir;

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
        fs::write(
            skills_dir.join("Orphan").join("SKILL.md"),
            "---\nname: Orphan\ndescription: no manifest\n---\n",
        )
        .unwrap();
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
        fs::write(
            skills_dir.join("Tracked").join("SKILL.md"),
            "---\nname: Tracked\ndescription: t\n---\n",
        )
        .unwrap();
        fs::create_dir_all(skills_dir.join("Untracked")).unwrap();
        fs::write(
            skills_dir.join("Untracked").join("SKILL.md"),
            "---\nname: Untracked\ndescription: u\n---\n",
        )
        .unwrap();

        let manifest_path = skills_dir.join(".skills.json");
        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
        manifest.add_skill(
            "Tracked",
            SkillEntry::new("test/repo", "", &testutil::make_fm("Tracked"), "abc", "url"),
        );
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
        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
        manifest.add_skill(
            "Ghost",
            SkillEntry::new("test/repo", "", &testutil::make_fm("Ghost"), "abc", "url"),
        );
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
        let manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };

        let untracked = scan_untracked_skills(&skills_dir, &manifest).unwrap();
        assert!(untracked.is_empty());
    }

    #[test]
    fn test_scan_untracked_skips_dot_dirs() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join(".hidden")).unwrap();
        fs::write(
            skills_dir.join(".hidden").join("SKILL.md"),
            "---\nname: Hidden\ndescription: d\n---\n",
        )
        .unwrap();
        let manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };

        let untracked = scan_untracked_skills(&skills_dir, &manifest).unwrap();
        assert!(untracked.is_empty());
    }

    #[test]
    fn test_scan_untracked_skips_dirs_without_skill_md() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("no-skill")).unwrap();
        fs::write(skills_dir.join("no-skill").join("README.md"), "not a skill").unwrap();
        let manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };

        let untracked = scan_untracked_skills(&skills_dir, &manifest).unwrap();
        assert!(untracked.is_empty());
    }

    #[test]
    fn test_scan_untracked_tracks_by_frontmatter_name() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(skills_dir.join("dir-name")).unwrap();
        fs::write(
            skills_dir.join("dir-name").join("SKILL.md"),
            "---\nname: FmName\ndescription: d\n---\n",
        )
        .unwrap();
        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
        // Track by frontmatter name (not dir name)
        manifest.add_skill(
            "FmName",
            SkillEntry::new("test/repo", "", &testutil::make_fm("FmName"), "abc", "url"),
        );

        let untracked = scan_untracked_skills(&skills_dir, &manifest).unwrap();
        assert!(untracked.is_empty());
    }

    #[test]
    fn test_output_json_with_both_types() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        let manifest_path = skills_dir.join(".skills.json");

        let mut manifest = Manifest {
            version: 1,
            skills: std::collections::HashMap::new(),
        };
        manifest.add_skill(
            "Tracked",
            SkillEntry::new("test/repo", "", &testutil::make_fm("Tracked"), "abc", "url"),
        );
        manifest.save(&manifest_path).unwrap();

        fs::create_dir_all(skills_dir.join("Manual")).unwrap();
        fs::write(
            skills_dir.join("Manual").join("SKILL.md"),
            "---\nname: Manual\ndescription: m\n---\n",
        )
        .unwrap();

        let result = execute(true, &skills_dir, &manifest_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_display_version_empty() {
        assert_eq!(display_version(""), "-");
    }

    #[test]
    fn test_display_version_legacy_zero() {
        // Backward compat: "0.0.0" stored by older versions
        assert_eq!(display_version("0.0.0"), "-");
    }

    #[test]
    fn test_display_version_real() {
        assert_eq!(display_version("1.0.0"), "1.0.0");
        assert_eq!(display_version("2.3.4-beta"), "2.3.4-beta");
    }

    #[test]
    fn test_display_version_not_zero_prefix() {
        // Should not match partial "0.0.0" in longer strings
        assert_eq!(display_version("0.0.1"), "0.0.1");
        assert_eq!(display_version("10.0.0"), "10.0.0");
    }
}
