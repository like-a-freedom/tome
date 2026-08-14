# ADR-0008: Remove the ManifestStore Trait

## Status
Accepted (supersedes ADR-0006)

## Context
ADR-0006 extracted a 12-method `ManifestStore` trait with two implementations:
`impl ManifestStore for Manifest` (thin forwarding) and `MemoryManifestStore`
(in-memory test double). One year of production code later, the seam earned its
keep only in theory:

- Every production caller uses the concrete `Manifest` directly. The only trait
  method not already inherent on `Manifest` is `all_skills` (a 3-line
  `skills_map()` projection). The other 11 methods are one-line forwards to the
  inherent versions — trait-based dispatch over zero behavior.
- `MemoryManifestStore` has zero callers outside its own test module. The
  in-memory test double was never adopted: services need `load`/`save` (disk),
  so tests keep using real temp directories.
- `list.rs` and `export.rs` import `ManifestStore` purely to reach `all_skills`.
  Removing the trait leaves those call sites unchanged (they already hold a
  concrete `Manifest`).
- The audit found `resolve_names` and `find_stale` duplicated verbatim between
  the trait impl and `Manifest`, i.e. the trait was already the second copy of
  the behavior, not the first.

A trait with one production implementation, one test-only implementation, and
11 forwarding methods is a speculative seam: it costs a module, an import, and
a doc-comment promise per method without constraining or enabling any caller.

## Decision
Delete `src/core/manifest_store.rs` entirely:

- Move the one non-inherent method, `all_skills()`, onto `impl Manifest`
  (body unchanged: `skills_map().iter().map(...)`).
- Drop `use crate::core::manifest_store::ManifestStore` from `commands/list.rs`
  and `commands/export.rs` — `all_skills()` resolves to the inherent method.
- Remove `pub mod manifest_store;` from `src/core.rs`.
- Update ADR-0006's status to note it is superseded by this ADR.
- The test seam for manifest behavior remains temp-dir-backed integration tests
  (the pattern services already use).

## Consequences

### Positive
- One module and ~280 lines of dead weight gone (trait, forwarding impl,
  `MemoryManifestStore`, and its tests).
- No semantic change anywhere: all call sites resolve to the same method
  bodies.
- `resolve_names`/`find_stale` exist exactly once (on `Manifest`).

### Negative
- Loses the in-memory test double (unused in practice, and temp-dir tests were
  the actual pattern).
- Loses the written-down interface; `Manifest`'s public surface is now the
  de-facto contract. Acceptable for a single-struct aggregate in `core`.

### Compatibility with existing ADRs
- ADR-0003 (ManifestStore): the original file-based store vision — realized by
  `Manifest::load/save` itself; no further action.
- ADR-0006: superseded by this ADR.
- ADR-0009 (read-side scan): unaffected; the read-side primitive is a separate
  module with its own interface.

## Files Affected
- **DELETED** `src/core/manifest_store.rs` (trait + `MemoryManifestStore` + tests)
- `src/core.rs` — remove `pub mod manifest_store;`
- `src/core/manifest.rs` — add inherent `all_skills()` (+ tests)
- `src/commands/list.rs` — drop the `ManifestStore` import
- `src/commands/export.rs` — drop the `ManifestStore` import
- `docs/adr/ADR-0006-manifest-store-trait.md` — status note: superseded
