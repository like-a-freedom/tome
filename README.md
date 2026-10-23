# tome

[![CI](https://github.com/like-a-freedom/tome/actions/workflows/ci.yml/badge.svg)](https://github.com/like-a-freedom/tome/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Minimal CLI for managing AI agent skills in a centralized `.agents/skills/`
directory. Strictly compliant with the [SKILL.md specification](https://agentskills.my/specification/).

## Why

I tried several tools for managing agent skills. Most of them bundle a marketplace, a leaderboard, format translation, multi-agent sync, a TUI, a server, and sometimes telemetry on top. I wanted to do one thing: sync a chosen set of GitHub repos into a single `~/.agents/skills/` directory on my machine.

`tome` does that. Add a repo, list what is installed, update everything in one command, remove what you do not need. No symlink dance, no format translation, no remote service.

If you want one job done well, this is it.

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
[releases page](https://github.com/like-a-freedom/tome/releases).

## Usage

```bash
# Add a skill from a GitHub repository
tome add owner/repo

# Add a specific skill from a multi-skill repository
tome add owner/repo@skill-name

# Add a local file
tome add file:///path/to/SKILL.md

# List installed skills
tome list                  # searchable browser in a terminal
tome list --plain          # full table, without the browser
tome list --json

# Update all tracked skills
tome update
tome update my-skill        # targeted update
tome update --interactive  # choose exact skills; no new skills are installed

# Select skills to remove, then review and confirm with y
tome remove
tome remove my-skill
tome remove owner/repo --force  # explicit target; skip confirmation

# Export an install script for all tracked skills
tome export > install.sh
tome export --interactive > install.sh  # export only selected skills
```

Skills are installed to `~/.agents/skills/` by default. Override with
`SKILLS_DIR=/path/to/skills` or `tome --dir /path/to/skills <command>`.

## Working with large lists

The same searchable, paged list is used for installation, inventory, removal,
interactive update/export, and long operation reports. Lists sort by source and
skill name. The focused skill shows its source, version, status and description;
press `d` for full, scrollable details.

| Key | Action |
| --- | --- |
| Arrows / `j` / `k` | Move through skills |
| Page Up / Page Down / Home / End | Jump through the list or details |
| `/` | Search names, descriptions, sources, versions and statuses; all words must match |
| Esc while searching | Leave search, keeping the filter |
| Space / Enter while searching in a picker | Toggle the focused match / continue; Enter selects the focused match if nothing is selected |
| Alt-Space while searching | Insert a space between search words |
| Ctrl-U while searching | Clear the query |
| Space | Toggle the focused skill in a picker |
| `a` / `n` | Select / clear every matching skill, including matches outside the viewport |
| Tab | Show only selected skills; hidden selections are retained and counted |
| Enter in a picker | Continue with the selected skills |
| `d`, then Esc | Open full details, then return |
| `q` / Esc / Ctrl-C | Close or cancel; Ctrl-C also works during search |

Removal starts with nothing selected. Its review lists the entire deletion set;
searching the review does not narrow that set. Only `y` confirms deletion. Enter
cancels, and `b` returns to selection with the choices preserved. `--force`
requires an explicit skill, source or URL. Closing a report leaves the completed
operation intact.

Automatic browsing requires terminal stdin, stdout and stderr and a usable
`TERM` (not `dumb`). Pipes retain complete, line-based output; `list --plain` and
`list --json` always bypass the browser. Explicit `--interactive` update/export
requires terminal stdin and stderr and fails with an actionable error otherwise.
The picker renders on stderr, so redirected export stdout contains only commands.
`tome remove` without a target also requires an interactive terminal. Resize the
terminal to at least 24 columns and 11 rows to use list actions.

## File safety and failures

Add, update and remove stage filesystem changes with reversible backups until
`.skills.json` is saved using an atomic replacement. A copy failure preserves the
installed skill; a fatal error or failed manifest save rolls back the operation.
Update can still save successful skills when other sources fail, and returns a
nonzero exit status if any skill failed. A source changing a tracked skill's name
is rejected and leaves its installed content intact. Update checks the committed
Git tree before treating a missing checkout file as deleted. Tracked custom paths
omitted by sparse checkout use a full checkout fallback; broken source symlinks
produce a failure while preserving the installed skill.

Untracked paths, including dangling symlinks, are skipped without `add --force`.
Replacing or removing a skill symlink changes the link itself, preserving its
target. Downloaded `SKILL.md` temporary files are cleaned up after the operation.
These guarantees cover handled errors; crash recovery and concurrent mutations
from separate processes are not transactional.

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
