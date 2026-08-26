# tome

[![CI](https://github.com/solovey/tome/actions/workflows/ci.yml/badge.svg)](https://github.com/solovey/tome/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Minimal CLI for managing AI agent skills in a centralized `.agents/skills/`
directory. Strictly compliant with the [SKILL.md specification](https://agentskills.my/specification/).

## What

`tome` installs, lists, updates, removes, and exports AI agent skills stored as
`SKILL.md` files. Skills come from GitHub repositories or `file://` URLs, get
cloned sparsely, and live in one place that every agent on your machine can
read.

## Install

```bash
cargo install tome
```

Or grab a release binary from the
[releases page](https://github.com/solovey/tome/releases).

## Usage

```bash
# Add a skill from a GitHub repository
tome add owner/repo

# Add a specific skill from a multi-skill repository
tome add owner/repo@skill-name

# Add a local file
tome add file:///path/to/SKILL.md

# List installed skills
tome list
tome list --json

# Update all tracked skills
tome update
tome update my-skill        # targeted update

# Remove a skill
tome remove my-skill

# Export an install script for all tracked skills
tome export > install.sh
```

Skills are installed to `~/.agents/skills/` by default. Override with
`SKILLS_DIR=/path/to/skills` or `tome --dir /path/to/skills <command>`.

## Documentation

- [`docs/PRD.md`](docs/PRD.md) — product requirements
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — system architecture
- [`docs/adr/`](docs/adr/) — architecture decision records
- [`CONTEXT.md`](CONTEXT.md) — domain glossary

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). Quality gate is `cargo fmt --check &&
cargo clippy --all-targets -- -D warnings && cargo test`.

## License

MIT — see [`LICENSE`](LICENSE).
