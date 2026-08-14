# ADR-0009: Read-Side Skill Scan Module

## Status
Accepted

## Context
ADR-0001 deferred the read-side extraction: "list/export remain a deliberate
deferred read-side extraction" (`src/services.rs` header). The read-side
question — *"what counts as a skill on disk in the Skills Directory?"* — is
currently answered twice, with slightly different rules:

- `Manifest::rebuild_from_directory` (manifest.rs:174–227): iterates the skills
  dir, skips dot-directories, accepts any directory containing `SKILL.md`,
  validates names (bails on invalid), warns on unparseable frontmatter, and
  registers entries keyed by *directory name*.
- `commands/list.rs::scan_untracked_skills` (list.rs:63–116): same iteration
  and dot-filter, then silently skips unparseable frontmatter, checks
  tracked-by-dir-name and tracked-by-frontmatter-name, and reports entries by
  *frontmatter name*.

Two modules therefore own "is this directory a skill", and their policies
(keying, validation, warnings) differ. The audit's Candidate 1 is to extract
the shared primitive: a flat, one-level scan of the Skills Directory that
returns, per directory, the directory name plus the parsed frontmatter, and
lets each caller apply its own policy on top.

## Decision
Add `src/core/scan.rs` with one primitive:

```rust
/// A skill directory discovered in the Skills Directory.
pub struct ScannedSkill {
    pub dir_name: String,
    pub frontmatter: SkillFrontmatter,
}

/// One-level scan of `skills_dir`: every non-hidden, non-skipped directory
/// containing a parseable SKILL.md, in readdir order.
pub fn scan_skills_dir(skills_dir: &Path) -> Result<Vec<ScannedSkill>>;
```

Rules owned by the primitive (the single definition of "skill on disk"):
- one level deep (no recursion — nested skill directories are not skills of
  the Skills Directory);
- directory must exist and contain `SKILL.md`;
- dot-directories and `SKIP_DIRS` entries are skipped;
- unparseable frontmatter emits a warning to stderr and skips the directory
  (the warning text currently used by `rebuild_from_directory`).

Policy stays with the callers:
- `Manifest::rebuild_from_directory` = scan + validate both names + key by
  directory name (current behavior, including error propagation).
- `list.rs::scan_untracked_skills` = scan + filter tracked-by-dir-name and
  tracked-by-frontmatter-name + map to `UntrackedSkill`.

The scan primitive deliberately does NOT touch repo-side geometry
(`Skills/<agent>/<skill>/SKILL.md` layouts, subdirectory precedence) — that is
`repo_detect`'s domain, shared with sparse checkout via ADR-0007.

## Consequences

### Positive
- Single definition of "what counts as a skill on disk"; `rebuild` and
  `scan_untracked_skills` cannot drift apart again.
- `rebuild_from_directory` shrinks to policy; `scan_untracked_skills` loses
  its duplicated fs loop.
- `list` now warns on unparseable frontmatter instead of silently dropping the
  directory (diagnostic parity with rebuild).

### Negative
- New module (~60 lines).
- `list` gains stderr warnings for invalid frontmatter where it was previously
  silent (accepted: it is a diagnostic improvement).
- Read order remains filesystem order; dedup by name stays a caller policy.

### Compatibility with existing ADRs
- ADR-0001: realizes the deferred read-side extraction (the scan primitive is
  the read-side interface; commands still apply presentation policy).
- ADR-0006/0008: `rebuild_from_directory` stays on `Manifest` (it is a
  manifest-construction policy, not a store query).
- ADR-0007: no overlap; repo geometry stays in `repo_detect`.

## Files Affected
- **NEW** `src/core/scan.rs` (primitive + tests)
- `src/core.rs` — register `pub mod scan;`
- `src/core/manifest.rs` — `rebuild_from_directory` delegates to scan
- `src/commands/list.rs` — `scan_untracked_skills` delegates to scan