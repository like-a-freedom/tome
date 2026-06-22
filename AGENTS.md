# tome

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
cargo run -- list
cargo run -- list --json
cargo run -- remove skill-name
cargo run -- update
cargo run -- update --verbose
cargo run -- update my-skill
SKILLS_DIR=/path/to/skills cargo run -- list
```

## Code style

- Rust edition 2021, 2018+ module style (no `mod.rs`)
- All functions return `anyhow::Result<T>` with `.context()` for errors
- Named constants for all numeric literals (no magic numbers)
- Unit tests in `#[cfg(test)] mod tests` blocks within each source file
- Shared test helpers in `src/testutil.rs` (lib) and `tests/common/mod.rs` (integration)
- Integration tests in `tests/integration.rs`
- `clap` derive for CLI args, `serde` for serialization

## Architecture

```ascii
main.rs → lib.rs::run() → commands/{add,list,remove,update}
                           core/{spec,manifest,git,normalize,parse_source}
                           fs.rs (temp dirs, directory copy)
                           style.rs (ANSI helpers, box-drawing)
                           tui.rs (interactive multi-select)
```

Key design decisions in `docs/DESIGN.md`, full architecture in `docs/ARCHITECTURE.md`.

## Project scope

- GitHub public repos only (v1)
- SKILL.md format only, no translation
- `.agents/skills/` as single source of truth, `.skills.json` manifest
- No telemetry, no server, no configuration beyond `.skills.json`
