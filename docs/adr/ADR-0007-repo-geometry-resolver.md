# ADR-0007: Repo-Geometry Resolver in `repo_detect`

## Status
Accepted

## Context
`detect_repo_type` answers "what kind of repository is this" (`SingleSkillAtRoot`,
`SingleSkillInDir`, `MultiSkill`), but every consumer re-implements the follow-up question
"which skills actually live here, and where".

- `SkillService::install` re-derives selection (requested-name match, TUI multi-select) inside
  a `match repo_type`, inline in the service.
- `UpdateService` needs the inverse operation — given a clone and an empty recorded path, find
  the sole skill directory — and hand-rolls it in `skill_repo_path`, even re-running the same
  scan `detect_repo_type` already performed.

The same repo-geometry knowledge is now encoded three times, and `update_service.rs` imports
`RepoType` only to pattern-match a fallback. This is a small slice of ADR-0005's deferred
domain model: repository geometry is the one piece of domain behavior both services already
need.

## Decision
Give `repo_detect` two query functions that answer the geometry questions directly, and route
both services through them:

```rust
/// All skills in the repository, root SKILL.md first, then the first
/// populated skills subdirectory, then top-level skill directories.
pub fn resolve_skills(repo_path: &Path) -> Result<Vec<SkillInfo>>;

/// The path of the single skill in the repository, or the repository root
/// when no single skill can be pinned down (legacy fallback).
pub fn sole_skill_dir(repo_path: &Path) -> Result<PathBuf>;
```

- `resolve_skills` returns a flat, root-first list so the caller can apply its own policy
  (single vs. multi, requested-name matching, interactive selection). RepoType precedence rules
  are preserved: root SKILL.md always first; a populated subdirectory wins over top-level dirs.
- `sole_skill_dir` reproduces the current `skill_repo_path` fallback exactly: a single root
  skill or single in-dir skill resolves to that skill's path; any multi-skill repo falls back to
  the repository root. It stays within the clone via `fs::path_within`.
- `detect_repo_type` remains the classifier and keeps its full test suite; the new functions
  are built on the same primitives (`find_skills_in_subdir`, `find_top_level_skills`).

## Consequences

### Positive
- Single source of truth for repo-geometry queries; `install` and `update` stop each encoding
  their own variant of the same rules.
- `SkillService::install` shrinks to a policy loop over `resolve_skills` (requested-name match,
  multi-select, validation) — easier to read and to test.
- `update_service.rs` drops its `RepoType` import and the duplicated scan.
- Progresses ADR-0005's "resolution logic re-implemented in every command" complaint without
  opening the broader domain-model work.

### Negative
- `resolve_skills` and `detect_repo_type` share scan primitives; the two must not drift (kept in
  one module, so the overlap is visible and small).
- Slightly larger public surface on `repo_detect` (two new pub fns).

### Compatibility with existing ADRs
- ADR-0001: no impact (service layer still the caller; commands unchanged).
- ADR-0005: a first concrete step of the deferred domain model; does not require the rest.
- ADR-0006: no overlap (manifest seam untouched).

## Files Affected
- `src/core/repo_detect.rs` — add `resolve_skills` + `sole_skill_dir` (+ tests)
- `src/services/skill_service.rs` — selection via `resolve_skills`
- `src/services/update_service.rs` — `skill_repo_path` → `sole_skill_dir`; drop `RepoType` import
