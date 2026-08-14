# ADR-0010: Presentation State as Arguments, Not Policy Queries

## Status
Accepted

## Context
`UpdateReporter` is the event interface between `UpdateService` (domain) and
`UpdateProgress` (presentation). One method breaks the direction of that
relationship:

```rust
fn is_live(&self) -> bool;   // update_service.rs:61
```

`UpdateService::update_git_group` calls `reporter.is_live()` (update_service.rs:348)
to decide whether to use the progress-tracking clone
(`clone_repo_or_sparse_tracked`) or the streaming clone (`GitOutput::Progress`).
Two problems:

1. **The domain queries the presentation for a presentation fact.** "Is the
   terminal live?" is not a domain question; it is a property of the caller's
   environment. The service cannot know it without asking the UI — and every
   `UpdateReporter` implementer must answer it (the test `RecordingReporter`
   stubs it with `false`, update_service.rs:631).
2. **The trait's contract is polluted.** An observer interface that reports
   events now also answers policy questions, so every implementer carries an
   extra method and the interface has two different roles.

The same class of problem appears in miniature at `UpdateProgress`: 7 of its 8
inherent methods exist only to forward to the `UpdateReporter` trait impl
(progress.rs:270–304), so the inherent impl and the trait impl can drift in
signature and behavior.

## Decision
1. **Remove `is_live` from `UpdateReporter`.** `UpdateService::run` gains a
   `live: bool` parameter; `update_git_group` receives it and selects the
   clone strategy. The caller — `commands/update.rs` — computes liveness once
   from the presentation object it already owns (`UpdateProgress::enabled()`).
2. **Merge `UpdateProgress`'s inherent event methods into the trait impl.**
   `start`, `repo_started`, `phase`, `skill_checked`, `clone_progress`, and
   `repo_finished` exist exactly once, as trait methods. `new`, `enabled`, and
   `finish` stay inherent (`new`/`enabled` are construction/presentation
   queries for the caller; `finish` is not part of the reporter event stream).

## Consequences

### Positive
- The domain no longer depends on the UI to know its environment: `live` is a
  plain argument, testable with a literal `false`.
- `UpdateReporter` is a pure observer again (start/repo_started/phase/
  skill_checked/clone_progress/repo_finished).
- ~35 lines of forwarding removed from `progress.rs`; the trait impl is the
  only home of event logic.
- Any future reporter (log-file, JSON) is free of terminal queries.

### Negative
- `UpdateService::run` and `update_git_group` gain a `bool` parameter — a
  small, explicit cost for removing hidden coupling.
- `UpdateProgress` tests must import `UpdateReporter` to call event methods.

### Compatibility with existing ADRs
- ADR-0006/0008: unrelated (manifest seam).
- ADR-0007: unrelated (repo geometry).
- ADR-0001: strengthens the service layer's role — services take context as
  arguments, never by asking the presentation layer.

## Files Affected
- `src/services/update_service.rs` — remove `is_live` from trait and
  `RecordingReporter`; add `live: bool` to `run` and `update_git_group`
- `src/commands/update.rs` — compute `live` via `UpdateProgress::enabled()`;
  pass to `run`
- `src/progress.rs` — trait impl takes the real bodies; `is_live` removed