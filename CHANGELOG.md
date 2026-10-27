# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

[0.10.0]: https://github.com/like-a-freedom/tome/compare/v0.9.1...v0.10.0
[0.9.1]: https://github.com/like-a-freedom/tome/compare/v0.9.0...v0.9.1
