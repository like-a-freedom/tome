# tome workflows

## Batch-remove from the inventory

1. `tome list`
2. `/grafana` to filter, then `a` to mark every match — or Space per row.
3. Check the footer count and press `x`. Removal is immediate; the notice
   reports the count.

Single skill: focus the row and press `x` with nothing marked.

## Keep removed skills removed

- Removing a subset marks the source partial; `tome update` skips new-skill
  adoption for partial sources.
- Bring one skill back with `tome add owner/repo@name` (source stays partial),
  or re-add the whole repo to clear the flag and resume adoption.
- Removing every skill of a source drops it from update entirely.

## Reproducible machines

`tome export > install.sh`, then `sh install.sh` on the next machine.
`--interactive` exports only the picked subset.

## Stale and untracked skills

- `list` warns about manifest entries missing on disk; `tome remove <name>`
  cleans them up.
- Manually placed skills are untracked: visible in list, never mutated.
  Remove them with `rm -rf`.

## Troubleshooting

- Pipes and CI get line-based output; use `list --json` in scripts.
- "Interactive selection requires a terminal": run in a TTY or pass explicit
  names.
- `--force` without a name fails by design.
- Default sparse checkout fetches only skill paths; `--no-sparse` when you need
  the full repo.
