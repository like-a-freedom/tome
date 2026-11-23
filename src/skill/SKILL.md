---
name: tome
description: "Manual for the tome CLI: install, list, update, remove, and export AI agent skills stored as SKILL.md directories. Use when running or scripting tome, choosing flags, driving the terminal browser, or managing tracked sources in .agents/skills."
metadata:
  version: "1.0"
---
# tome

tome keeps agent skills as plain `SKILL.md` directories in `~/.agents/skills`.
The `.skills.json` manifest in that directory is the single source of truth.
Override the directory with `SKILLS_DIR` or `tome --dir <path>`.
Sources: public GitHub repos and `file://` local repos. No telemetry, no server.

## Quick start

```sh
tome add owner/repo          # install every skill of the repo
tome add owner/repo@name     # install one skill
tome add file:///path/repo   # local repo
tome list                    # inventory browser: Space marks, x removes
tome update                  # refresh tracked sources, adopt new repo skills
tome remove name             # review, then remove; --force skips review
tome export > install.sh     # print reinstall commands
tome skill                   # this manual
```

## Rules that change behavior

- Untracked directories show in `list` but tome never updates or removes them.
- Removing part of a repo marks the source partial: `update` will not reinstall
  the removed skills. A full `tome add owner/repo` restores adoption.
- Without a TTY, commands print line-based output; `list --json` and
  `list --plain` always bypass the browser.

## Go deeper

- `tome skill commands` — arguments and flags per command.
- `tome skill tui` — browser modes and keys.
- `tome skill workflows` — batch removal, partial sources, export, fixes.
