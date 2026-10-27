# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.10.1] - 2026-10-27

### Fixed
- `tome add` now clears the `partial` flag once the selection tracks every
  installable skill in the repository. Previously a full re-selection over
  already installed skills (which the installer skips) or re-adding the last
  missing skill of a fully tracked repository left new-skill sync silently
  disabled forever. Run `tome add <repo>` and select every skill to restore
  sync.
- Uninstallable upstream skills (invalid names such as `bad/name`) no longer
  count towards completeness, so a repository containing one can still reach
  the fully tracked state.
- A picker returning duplicate entries for a partial selection is no longer
  mistaken for a full selection: `partial` is decided by comparing skill sets,
  not entry counts.

### Changed
- Documentation (AGENTS.md, CONTEXT.md, docs/ARCHITECTURE.md, docs/DESIGN.md)
  states that any partial entry disables new-skill adoption for its whole
  source group and that `tome add` clears it when the repository is fully
  tracked.

## [0.10.0] - 2026-10-27

### Added
- `SkillEntry.partial` manifest flag, set when only a subset of a multi-skill
  repository is installed (picker selection, `@skill-name`, or `--path`).

### Fixed
- Stop `tome update` from silently adopting skills the user declined during a
  partial install. Untargeted updates now skip new-skill sync for any group
  containing a partial entry; fully tracked groups still adopt skills that
  genuinely appear upstream. Older manifests load with the flag off, so
  existing full-selection installs keep syncing.

## [0.9.1] - 2026-10-23

### Fixed
- Allow selecting filtered skills with Space and confirming with Enter.
- Allow switching to selected skills with Tab while searching.
- Prevent repeated key events from toggling selections or confirming dialogs.
- Support spaces in search queries with Alt-Space.

[0.10.1]: https://github.com/like-a-freedom/tome/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/like-a-freedom/tome/compare/v0.9.1...v0.10.0
[0.9.1]: https://github.com/like-a-freedom/tome/compare/v0.9.0...v0.9.1
