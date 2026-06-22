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
```

Both must pass clean before any commit or PR.

### Documentation

Use `context7` MCP to query library documentation (e.g. crossterm, clap, serde).
Use `keenable` MCP for web search when context7 has no match.

## Run

```bash
cargo run -- add owner/repo
cargo run -- add owner/repo@skill-name
cargo run -- list
cargo run -- remove skill-name
cargo run -- update
SKILLS_DIR=/path/to/skills cargo run -- list
```

## Code style

- Rust edition 2021, 2018+ module style (no `mod.rs`)
- All functions return `anyhow::Result<T>` with `.context()` for errors
- Unit tests in `#[cfg(test)] mod tests` blocks within each source file
- Integration tests in `tests/integration.rs`
- `clap` derive for CLI args, `serde` for serialization

## Architecture

```ascii
main.rs → lib.rs::run() → commands/{add,list,remove,update}
                           core/{spec,manifest,git,normalize,parse_source}
                           fs.rs (temp dirs, directory copy)
```

Key design decisions in `docs/DESIGN.md`, full architecture in `docs/ARCHITECTURE.md`.

## Project scope

- GitHub public repos only (v1)
- SKILL.md format only, no translation
- `.agents/skills/` as single source of truth, `.skills.json` manifest
- No telemetry, no server, no configuration beyond `.skills.json`
