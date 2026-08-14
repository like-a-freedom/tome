# Tome Architecture

## Overview

`tome` is a small Rust 2024 CLI for managing AI-agent skills in a centralized
`.agents/skills` directory. The application is intentionally file-based and
uses the Git CLI rather than a native Git library.

## Technology

| Area | Implementation |
|---|---|
| CLI parsing | `clap` derive with environment support |
| Error handling | `anyhow::Result` with contextual errors |
| Manifest serialization | `serde` and `serde_json` |
| Skill frontmatter | `serde_yaml` behind a small preprocessing step |
| Timestamps | `chrono` |
| Temporary files | `tempfile` |
| Terminal UI | `crossterm` |
| Git | `std::process::Command` wrapping the Git CLI |

Git remains a CLI dependency by design: it keeps the binary small and avoids
native `libgit2` build and linking requirements.

## Repository Structure

The crate follows the modern 2018+ module layout. `services.rs` declares the
`services/` child modules; there is deliberately no `services/mod.rs`.

```text
tome/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs                 # thin argument-parsing entry point
│   ├── lib.rs                  # application root and command dispatch
│   ├── commands.rs             # command module declarations
│   ├── commands/
│   │   ├── add.rs              # CLI adapter → SkillService
│   │   ├── remove.rs           # CLI adapter → SkillService
│   │   ├── update.rs           # CLI adapter → UpdateService + presentation
│   │   ├── list.rs             # read-side list rendering and scanning
│   │   └── export.rs           # read-side export rendering
│   ├── services.rs             # application-service module declaration
│   ├── services/
│   │   ├── skill_service.rs    # install/remove orchestration
│   │   └── update_service.rs   # update orchestration and reporter seam
│   ├── core.rs                 # core module declarations
│   ├── core/
│   │   ├── commit.rs           # commit short-form and comparison helpers
│   │   ├── dirs.rs             # shared scan constants and no-skill message
│   │   ├── git.rs              # Git clone, sparse checkout, and metadata
│   │   ├── git/
│   │   │   └── git_sparse.rs   # nested `.agents/skills/` sparse-checkout fetcher
│   │   ├── install.rs          # resolved skills, copy, manifest updates
│   │   ├── manifest.rs         # .skills.json persistence and lookup
│   │   ├── parse_source.rs     # source parsing and labels
│   │   ├── repo_detect.rs      # repository skill discovery and geometry
│   │   ├── scan.rs             # shared SKILL.md directory scanning
│   │   └── spec.rs             # SKILL.md frontmatter parsing and validation
│   ├── config.rs               # CLI shape and path resolution
│   ├── fs.rs                   # filesystem helpers
│   ├── output.rs               # shared terminal output formatting
│   ├── progress.rs             # crossterm progress presentation
│   ├── style.rs                # ANSI and box-drawing helpers
│   ├── tui.rs                  # interactive multi-select
│   └── testutil.rs             # test-only shared fixtures
└── tests/
    ├── integration.rs          # public CLI regression tests
    └── common/mod.rs           # integration-test Git fixtures
```

## Layer Responsibilities

### `main.rs` and `lib.rs`

`main.rs` parses `Cli` and calls `tome::run`. `lib.rs` resolves the skills and
manifest paths, then dispatches to one command adapter. It does not contain
command business rules.

### Command adapters

Mutation commands follow this shape:

```text
CLI arguments → application service → output formatting
```

- `commands/add.rs` constructs `SkillService`, calls `install`, and renders
  `InstallResult`.
- `commands/remove.rs` resolves the removal target through `SkillService`,
  prompts when required, then calls `SkillService::remove`.
- `commands/update.rs` constructs `UpdateService`, supplies the
  `UpdateReporter`, and renders the returned result.

`list` and `export` are read-side commands. Their orchestration remains local
because no second read backend or separate read model is currently required;
this is explicitly deferred in ADR-0001 and the refactoring plan.

### Application services

`SkillService` owns install and remove orchestration: source parsing, Git/file
source handling, repository detection, frontmatter validation, installation,
removal, and manifest updates. Both services receive their paths through
`services::ServiceConfig`. `SkillService::install` accepts an optional picker
closure that replaces the interactive TUI multi-select for multi-skill
repositories; commands pass `None` and tests inject a pure function.

`UpdateService` owns update orchestration: selecting manifest entries, grouping
Git sources, updating file URL sources, restoring missing local skills,
removing skills deleted from their source, installing skills that newly
appeared in a tracked repository, reconciling stale entries, and saving the
manifest. It emits reporter-neutral `ProgressPhase` and `RepoOutcome` events
through `UpdateReporter`. `progress.rs` is the terminal adapter for that seam.

### Core modules

Core modules contain the concrete file, process, parsing, and manifest
operations used by the services and read-side commands. The current design has
one concrete disk-backed manifest and one concrete Git backend, so the service
layer calls those modules directly.

## Data Flows

### Install

```text
tome add source
  → parse Source
  → clone Git repository or download file URL
  → detect one or more skills
  → picker closure or TUI multi-select for multi-skill repositories
  → validate SKILL.md frontmatter
  → copy the selected skill directory
  → update .skills.json
  → render InstallResult
```

Local `file://` repositories are used by regression tests; no test requires a
network GitHub endpoint.

### Update

```text
tome update [name]
  → load manifest and select entries
  → group Git entries by remote; keep file URLs separate
  → clone/download each source
  → compare commits or file bytes
  → copy changed content and reconcile source deletions
  → install skills that newly appeared in the repository (untargeted update)
  → save manifest once
  → render reporter events and final summary
```

The seven reporter phases are `Downloading`, `Cloning`, `Checkout`, `Inspecting`,
`Checking`, `Copying`, and `Saving`. Verbose output is rendered by the command adapter.

Interactive terminals render a single crossterm live line in the style of `docker pull`:
a Braille spinner, a 10-cell block progress bar, the current phase, `source i/n`, and a
per-source elapsed timer (`46.3s`, or `1m 35s` beyond a minute). The bar shows one
monotonic 0-100 overall scale per source that never moves backward: git clone percent
maps to 0-35, sparse checkout percent to 35-60, and skills checked vs. total to 60-100
(see ADR-0011). `sync_found` extends the skill total when new skills are discovered in
a tracked repository, keeping the count from overflowing. Completed sources print a
permanent `✔` (clean) or `✖` (had failures/removals) line with their summary.
`--verbose` keeps detailed per-skill output and lets git show its own progress. Non-TTY
output is line-based and does not emit live progress cursor controls.

### Remove

```text
tome remove name [--force]
  → resolve the name, source label, or URL through SkillService
  → reject untracked targets before prompting
  → confirm unless --force
  → remove all resolved skill directories
  → remove their manifest entries
```

### List and export

`list` loads or rebuilds the manifest, reports stale entries without deleting
them, scans for manually placed skills, and renders table or JSON output.
`export` loads the manifest, sorts entries by skill name, and prints equivalent
`tome add` commands.

## Deferred Architecture Decisions

The audit identified three possible future seams. They are intentionally not
implemented yet:

- **ADR-0003 — `ManifestStore`:** the trait and in-memory test double were
  removed; `Manifest` gained the inherent `all_skills` accessor that the trait
  used to provide. Revisit if a second manifest backend or a mock-driven
  manifest seam is needed. (Superseded by ADR-0006 and the C1/C2 refactors.)
- **ADR-0004 — `GitRepository`:** revisit when a second Git backend or a
  mock-driven Git seam is needed; Git CLI remains the selected backend.
- **ADR-0005 — domain model:** revisit when `Skill`, repository references,
  and manifest relationships gain behavior that is not well represented by the
  current validated structs. (Repo geometry partially realized by ADR-0007;
  ADR-0008 removes the ManifestStore trait, ADR-0009 centralizes read-side
  skill scanning, ADR-0010 passes presentation state as arguments. The
  file-URL commit marker used by `is_file_url` is defined in CONTEXT.md.)

Adding these interfaces now would increase indirection without a concrete
second adapter. They remain roadmap decisions, not unwired public APIs.

## Testing

Unit tests live next to the module they exercise. The service layer has direct
unit coverage for file installation, source-based removal, and invalid source
handling. Public CLI integration tests cover add, list, export, remove, and
update flows, including Git/file update regressions and sparse checkout.

The current quality gate is:

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
git diff --check
```

The latest run passed 286 unit tests, 26 integration tests, and all doctests.
