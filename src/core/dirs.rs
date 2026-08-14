/// Directories to skip when scanning for skills or collecting sparse-checkout paths.
/// Shared between repo_detect.rs (skill detection) and git.rs (sparse checkout).
pub(crate) const SKIP_DIRS: &[&str] = &[
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

/// Canonical skill-directory names, in priority order.
/// Used by sparse checkout (git.rs) and repo detection (repo_detect.rs).
pub(crate) const SKILL_SUBDIRS: &[&str] = &["skills", "Skills", ".claude/skills", ".agents/skills"];

/// Shared error message when no skill layout is found in a repository.
/// Used by repo_detect.rs (fs scan) and git_sparse.rs (tree scan).
pub(crate) const NO_SKILL_FOUND_MSG: &str = "No SKILL.md found in repository root, skills/, Skills/, .claude/skills/, .agents/skills/ directory, or top-level skill directories";
