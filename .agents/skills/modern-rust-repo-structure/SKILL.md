---
name: rust-repo-structure
description: "Idiomatic Rust repository layout for small crates, multi-binary projects, and Cargo workspaces. Use when adding or moving modules in tome (main/lib/commands/services/core/tui), creating binaries or workspace crates, configuring features or profiles, or reviewing Rust project structure."
metadata:
  version: "1.1"
  author: "rust-repo-structure-skill"
  tags: ["rust", "cargo", "repository", "layout", "workspace", "module-structure"]
user-invocable: true
---

# Rust Repository Structure

Layout rules for this crate. Generic patterns — multi-binary crates, virtual
workspaces, feature flags, profiles, reference repos — live in
`references/rust-layout-patterns.md`; load it when the task goes beyond a single crate.

## Core rules

- `main.rs` parses arguments and calls `lib.rs::run`. Keep it thin.
- `lib.rs` exports the public API; logic lives in modules.
- One responsibility per module; shared types and errors get their own.
- 2018+ module style: `foo.rs` + `foo/`, never `mod.rs` (this crate: edition 2024).
- Unit tests beside the code in `#[cfg(test)]`; integration tests in `tests/`.
- Features additive, `default = []`; build profiles only in the workspace root.

## This repo

```ascii
main.rs → lib.rs::run() → commands/{add,list,remove,update,export}
                          services.rs → services/{skill_service,update_service}
                          core/{manifest,git,install,scan,...}
                          tui.rs   — terminal presentation only
                          output.rs — shared formatting
```

- `commands/` adapt the CLI to services; no domain logic.
- `services/` own use-cases and mutations: validate first, then commit
  (e.g. `remove_selected` checks ownership before the filesystem transaction).
- `core/` owns the domain: manifest, git, install, scan.
- `tui.rs` renders and reads keys; it never touches the manifest or the disk.
  New interactive behavior = a `ListMode` variant plus an outcome the command acts on.
- New command: `commands/<name>.rs` + a service method + unit tests in both.
