# tome commands

## add

`tome add <source> [--path <dir>] [--force] [--no-sparse]`

- `source`: `owner/repo`, `owner/repo@skill`, or `file:///path` to a local repo
  or SKILL.md.
- Multi-skill repos open a picker in a TTY; up to 20 skills start preselected.
  Choosing a subset marks the source partial.
- `--path` installs from a subdirectory; on a multi-skill repo it is a partial
  selection.
- `--force` overwrites an existing skill directory.
- Clones are sparse by default; `--no-sparse` fetches the full repo.

## list

`tome list [--json | --plain]`

- TTY: the Manage browser — mark with Space, remove with x, in place.
- `--plain`: full table. `--json`: array of {name, version, source, tracked,
  description}.
- Rebuilds the manifest from disk when missing; reports stale entries without
  deleting them.

## remove

`tome remove [name] [--force]`

- No name: TTY picker, then review — `y` confirms, `b` back, Enter cancels.
- `name`: a skill, `owner/repo`, or URL; resolves to every matching manifest key.
- `--force` skips confirmation and requires a name.
- Removing part of a source marks the remaining entries partial, so update
  cannot re-adopt the removed skills.

## update

`tome update [name] [--interactive] [--verbose] [--no-sparse]`

- Refreshes tracked skills; one clone per git remote group.
- Adopts newly appeared repo skills only for fully tracked sources, and only
  when updating everything; a targeted update never adopts.
- `--interactive` picks exact skills; `--verbose` lists every result.

## export

`tome export [--interactive]`

- Prints sorted `tome add` commands for tracked skills.
- `--interactive` exports only the picked subset; picker output stays on
  stderr, so redirected stdout contains only commands.

## skill

`tome skill [commands|tui|workflows]`

- Prints this embedded manual; an unknown topic fails and lists the topics.

## Global

- `--dir <path>` or `SKILLS_DIR` override the skills directory.
- Interactive features need TTY stdin and stderr and `TERM` other than `dumb`.
