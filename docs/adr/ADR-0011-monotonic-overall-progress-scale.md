# ADR-0011: Monotonic Overall Progress Scale for Update Sources

## Status
Accepted

## Context
The live progress bar showed two unrelated scales: git's transfer percent during
cloning and a skills count after. Transitions jumped backward (cloning 100% →
inspecting 0/7), sync_new_repo_skills work was invisible, and skill counts could
overflow past their total (3/2). Users read the bar as "how far along is this
source" — it must never move backward and must cover all work.

## Decision
Each git source renders one monotonic 0-100 overall scale with three fixed
segments: clone percent maps to 0-35, sparse checkout percent to 35-60, and
checked/total skills to 60-100. Progress events only ever raise the value
(max-clamp), so the bar never regresses. `sync_found(count)` extends the total
when new skills are discovered so the count stays honest, and a `Checkout` phase
keeps the label truthful during sparse checkout. File-URL skills keep the same
scale (0→100 on completion).

## Considered Options
- **Per-phase resetting bars** (docker-style): honest per operation but the
  overall position resets between phases, reintroducing the backward-jump
  complaint.
- **Adaptive/weighted-by-time segments**: weights would change mid-run,
  contradicting monotonicity.

## Consequences
- Fixed weights (35/25/40) are arbitrary but constant; changing them is a
  one-line constant edit.
- During a slow sparse checkout the bar sits in 35-60 instead of reporting the
  source done.
