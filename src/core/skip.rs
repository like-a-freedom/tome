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
