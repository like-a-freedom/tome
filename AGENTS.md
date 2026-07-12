# tome

## Code Navigation Rules

ALWAYS use octocode MCP tools before reading files directly:
- Use `semantic_search` to find relevant code by meaning
- Use `view_signatures` to understand file structure
- Use `graphrag` to explore dependencies between files
- Use `structural_search` for AST-level pattern search (replaces grep/rg)

NEVER:
- Run grep, rg, find to locate code — use semantic_search instead
- Read entire files to understand structure — use view_signatures instead
- Guess file locations — use graphrag overview first

WORKFLOW for any task:
1. graphrag overview → understand project structure
2. semantic_search → find relevant files
3. view_signatures → inspect structure of found files
4. Read only specific sections if needed

Minimal CLI for managing AI agent skills in a centralized `.agents` directory.

## Build

```bash
cargo build              # debug build
cargo build --release    # release build (stripped, LTO'd, ~1 MB)
```

## Test

```bash
cargo test               # all unit tests
cargo test integration   # integration tests (requires git)
```

## Quality gate (mandatory)

Before claiming work is complete, **always** run:

```bash
cargo clippy --all-targets   # zero warnings required
cargo test                   # all tests must pass
cargo fmt -- --check         # formatting must be clean
```

All three must pass clean before any commit or PR.

### Documentation

Use `context7` MCP to query library documentation (e.g. crossterm, clap, serde).
Use `keenable` MCP for web search when context7 has no match.

## Run

```bash
cargo run -- add owner/repo
cargo run -- add owner/repo@skill-name
cargo run -- add owner/repo --no-sparse
cargo run -- add file:///path/to/local/repo
cargo run -- list
cargo run -- list --json
cargo run -- remove skill-name
cargo run -- update
cargo run -- update --verbose
cargo run -- update my-skill
cargo run -- update --no-sparse
SKILLS_DIR=/path/to/skills cargo run -- list
```

## Code style

- Rust edition 2024, 2018+ module style (no `mod.rs`)
- All functions return `anyhow::Result<T>` with `.context()` for errors
- Named constants for all numeric literals (no magic numbers)
- Unit tests in `#[cfg(test)] mod tests` blocks within each source file
- Shared test helpers in `src/testutil.rs` (lib) and `tests/common/mod.rs` (integration)
- Integration tests in `tests/integration.rs`
- `clap` derive for CLI args, `serde` for serialization

## Architecture

```ascii
main.rs → lib.rs::run() → commands/{add,list,remove,update}
                           core/{spec,manifest,git,normalize,parse_source,download}
                           fs.rs (temp dirs, directory copy)
                           progress.rs (compact update status)
                           style.rs (ANSI helpers, box-drawing)
                           tui.rs (interactive multi-select)
```

Key design decisions in `docs/DESIGN.md`, full architecture in `docs/ARCHITECTURE.md`.

## Project scope

- GitHub public repos only (v1)
- File URL support: `file:///path/to/local/repo`
- SKILL.md format only, no translation
- `.agents/skills/` as single source of truth, `.skills.json` manifest
- `.agents/skills/` bundle support: sparse-checkout fetches nested `.agents/skills/` when present
- Top-level skill directories: detects `skill-name/SKILL.md` at repo root
- `tome update` uses compact crossterm progress in interactive terminals and stays line-based in non-TTY output
- No telemetry, no server, no configuration beyond `.skills.json`
