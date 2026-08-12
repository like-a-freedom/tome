# ADR-0006: ManifestStore Trait Extraction

## Status
Accepted

## Context
`core::manifest.rs` (717 lines) is a god object owning five responsibilities:
1. Data structure (`HashMap<String, SkillEntry>`)
2. Persistence (`load`/`save`)
3. Queries (`is_tracked`, `resolve_names`, `find_stale`)
4. Rebuild from disk (`rebuild_from_directory` — 55 lines of filesystem scanning)
5. Mutation (`add_skill`, `remove_skill`, `update_skill`)

The `skills` field is `pub(crate)`, allowing any module to reach into the HashMap directly.
Commands `list` and `export` bypass the service layer by accessing `manifest.skills.iter()`.
Tests must create temp directories and write real JSON files because there's no in-memory seam.

This is the highest-leverage deepening opportunity in the codebase.
It directly unblocks ADR-0001's deferred read-side extraction and ADR-0003's original vision.

## Decision
Extract a `ManifestStore` trait providing the query/mutation interface callers actually need.
`Manifest` implements the trait directly (no wrapper struct). A `MemoryManifestStore` (in-memory
HashMap) provides a test double. The `skills` field becomes private; a `pub(crate) skills_map()`
accessor serves only the trait impl in the same crate.

```rust
pub trait ManifestStore {
    fn is_tracked(&self, name: &str) -> bool;
    fn get_skill(&self, name: &str) -> Option<&SkillEntry>;
    fn all_skills(&self) -> Vec<(&str, &SkillEntry)>;
    fn skill_names(&self) -> Vec<String>;
    fn add_skill(&mut self, name: &str, entry: SkillEntry);
    fn remove_skill(&mut self, name: &str) -> Option<SkillEntry>;
    fn update_skill(
        &mut self,
        name: &str,
        commit: &str,
        version: &str,
        branch: &str,
        description: String,
    );
    fn is_empty(&self) -> bool;
    fn resolve_names(&self, input: &str) -> Vec<String>;
    fn find_stale(&self, skills_dir: &Path) -> Vec<String>;
    fn save(&self, path: &Path) -> Result<()>;
}
```

`rebuild_from_directory` remains a method on `Manifest` (not on the trait): it is a filesystem
adapter concern used only by the disk-backed read path (`commands/list.rs`), not by the trait's
callers. Services continue to use the concrete `Manifest` (they need `load`/`save`); the trait is
the seam for read-side callers and tests.

## Consequences

### Positive
- Callers interact through a trait, not `pub(crate)` fields — encapsulation restored
- `MemoryManifestStore` enables in-memory testing without temp dirs or JSON files
- Read-side service extraction (Candidate 1) becomes straightforward
- `MemoryManifestStore::update_skill` mirrors `Manifest::update_skill` semantics (refreshes the
  `updated` timestamp), so test doubles stay behaviorally aligned with the real adapter

### Negative
- New module `core/manifest_store.rs` (~120 lines)
- Requires updating all direct `manifest.skills` access sites (list.rs, export.rs)

### Compatibility with existing ADRs
- ADR-0001: directly enables deferred read-side extraction
- ADR-0003: implements the original ManifestStore vision (was deferred)
- ADR-0005: domain model gains a proper seam for `Manifest`

## Files Affected
- **NEW** `src/core/manifest_store.rs` (~120 lines)
- `src/core/manifest.rs` — `skills` field becomes private; `skills_map()` added as `pub(crate)` accessor
- `src/commands/list.rs` — use `ManifestStore` instead of `manifest.skills.iter()`
- `src/commands/export.rs` — use `ManifestStore::all_skills()` instead of `manifest.skills.iter()`
