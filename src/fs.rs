use anyhow::{Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn create_temp_dir() -> Result<tempfile::TempDir> {
    tempfile::tempdir().context("Failed to create temp directory")
}

pub fn ensure_skills_dir(path: &Path) -> Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path).context("Failed to create skills directory")?;
    }
    Ok(())
}

pub fn remove_skill_dir(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_dir_all(path).context("Failed to remove skill directory")?;
    }
    Ok(())
}

pub fn resolve_path_within(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative.contains('\\')
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        anyhow::bail!("Path must remain within its source directory: {relative}");
    }

    let candidate = root.join(relative_path);
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("Failed to resolve source root {}", root.display()))?;
    let mut existing_ancestor = candidate.as_path();
    while !existing_ancestor.exists() {
        existing_ancestor = existing_ancestor
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Path has no source ancestor: {relative}"))?;
    }
    let canonical_ancestor = existing_ancestor.canonicalize().with_context(|| {
        format!(
            "Failed to resolve source path {}",
            existing_ancestor.display()
        )
    })?;
    if !canonical_ancestor.starts_with(&canonical_root) {
        anyhow::bail!(
            "Path {} escapes source directory {}",
            candidate.display(),
            root.display()
        );
    }

    if candidate.exists() {
        path_within(root, &candidate)?;
        Ok(candidate)
    } else {
        Ok(candidate)
    }
}

pub fn path_within(root: &Path, candidate: &Path) -> Result<PathBuf> {
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("Failed to resolve source root {}", root.display()))?;
    let canonical_candidate = candidate
        .canonicalize()
        .with_context(|| format!("Failed to resolve source path {}", candidate.display()))?;

    if !canonical_candidate.starts_with(&canonical_root) {
        anyhow::bail!(
            "Path {} escapes source directory {}",
            candidate.display(),
            root.display()
        );
    }

    Ok(canonical_candidate)
}

pub fn copy_skill(src: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        std::fs::remove_dir_all(dest).context("Failed to remove existing skill")?;
    }

    copy_dir_recursive(src, dest).context("Failed to copy skill directory")?;

    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        #[cfg(unix)]
        if entry.file_type().is_ok_and(|t| t.is_symlink()) {
            let target = std::fs::read_link(&src_path)
                .with_context(|| format!("Failed to read symlink {}", src_path.display()))?;
            std::os::unix::fs::symlink(&target, &dest_path).with_context(|| {
                format!(
                    "Failed to create symlink {} -> {}",
                    dest_path.display(),
                    target.display()
                )
            })?;
            continue;
        }

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
    use tempfile::TempDir;

    fn make_skill_md(dir: &Path, name: &str, desc: &str) {
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {}\ndescription: {}\n---\n", name, desc),
        )
        .unwrap();
    }

    #[test]
    fn test_create_temp_dir() {
        let temp = create_temp_dir().unwrap();
        assert!(temp.path().exists());
        let file = temp.path().join("test.txt");
        std::fs::write(&file, "data").unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "data");
    }

    #[test]
    fn test_resolve_path_within_rejects_escape_and_absolute_paths() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir_all(root.join("nested")).unwrap();

        assert!(resolve_path_within(&root, "../outside").is_err());
        assert!(resolve_path_within(&root, root.to_str().unwrap()).is_err());
        assert!(resolve_path_within(&root, "nested").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn test_path_within_rejects_symlink_escape() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("root");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();

        assert!(resolve_path_within(&root, "link").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn test_resolve_path_within_keeps_root_basis_when_candidate_exists() {
        // Simulates macOS `/var` -> `/private/var`: the input root is a symlink
        // to the real directory, so canonicalizing the result would change its
        // basis and break callers that strip the root prefix.
        let temp = TempDir::new().unwrap();
        let real_root = temp.path().join("real");
        let link_root = temp.path().join("link");
        std::fs::create_dir_all(real_root.join("skills/custom")).unwrap();
        std::os::unix::fs::symlink(&real_root, &link_root).unwrap();

        let resolved = resolve_path_within(&link_root, "skills/custom").unwrap();
        assert_eq!(
            resolved,
            link_root.join("skills/custom"),
            "result must stay on the input root's basis, got: {:?}",
            resolved
        );
    }

    #[test]
    fn test_ensure_skills_dir_creates() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("agents").join("skills");
        ensure_skills_dir(&skills_dir).unwrap();
        assert!(skills_dir.exists());
    }

    #[test]
    fn test_ensure_skills_dir_already_exists() {
        let temp = TempDir::new().unwrap();
        let skills_dir = temp.path().join("agents").join("skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        let result = ensure_skills_dir(&skills_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_remove_skill_dir() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("some-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: x\ndescription: x\n---\n",
        )
        .unwrap();

        assert!(skill_dir.exists());
        remove_skill_dir(&skill_dir).unwrap();
        assert!(!skill_dir.exists());
    }

    #[test]
    fn test_remove_skill_dir_not_exists() {
        let temp = TempDir::new().unwrap();
        let result = remove_skill_dir(&temp.path().join("nonexistent"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_copy_skill_directory() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src-skill");
        std::fs::create_dir_all(&src).unwrap();
        make_skill_md(&src, "copied", "A copied skill");
        std::fs::create_dir(src.join("scripts")).unwrap();
        std::fs::write(src.join("scripts").join("run.sh"), "#!/bin/sh\necho hi").unwrap();

        let dest = temp.path().join("dest-skill");
        copy_skill(&src, &dest).unwrap();

        assert!(dest.join("SKILL.md").exists());
        assert!(dest.join("scripts").join("run.sh").exists());
    }

    #[test]
    fn test_copy_skill_overwrites_existing() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        make_skill_md(&src, "new", "new version");

        let dest = temp.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("SKILL.md"), "old content").unwrap();

        copy_skill(&src, &dest).unwrap();
        let content = std::fs::read_to_string(dest.join("SKILL.md")).unwrap();
        assert!(content.contains("new version"));
    }

    #[test]
    fn test_copy_dir_recursive_with_nested_dirs() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        std::fs::create_dir_all(src.join("a/b/c")).unwrap();
        std::fs::write(src.join("a/b/c/file.txt"), "deep").unwrap();

        let dest = temp.path().join("dest");
        copy_dir_recursive(&src, &dest).unwrap();
        assert!(dest.join("a/b/c/file.txt").exists());
    }

    #[test]
    fn test_copy_dir_recursive_with_symlink() {
        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("target.txt"), "content").unwrap();
        std::os::unix::fs::symlink("target.txt", src.join("link.txt")).unwrap();

        let dest = temp.path().join("dest");
        copy_dir_recursive(&src, &dest).unwrap();
        assert!(dest.join("link.txt").exists());
        assert_eq!(
            std::fs::read_link(dest.join("link.txt")).unwrap(),
            std::path::Path::new("target.txt")
        );
    }
}
