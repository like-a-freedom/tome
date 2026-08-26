# Contributing

Thanks for your interest in `tome`! This is a small, focused CLI — small
changes are easiest to review and merge.

## Process

1. Fork the repository.
2. Create a feature branch: `git checkout -b my-change`.
3. Make your changes.
4. Run the quality gate locally (see below). All three must pass clean.
5. Open a pull request against `main`.

## Quality Gate

Before pushing, run:

```bash
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

All three commands must exit with zero warnings/errors/failures. The CI workflow
runs the same checks on every push and PR.

## Code Style

- Rust edition 2024, 2018+ module layout (no `mod.rs`).
- All fallible functions return `anyhow::Result<T>`; chain `.context(...)` for
  errors.
- Named constants for numeric literals — no magic numbers in code.
- Unit tests live in `#[cfg(test)] mod tests` blocks within the source file
  they test. Integration tests live in `tests/integration.rs`.

See [`AGENTS.md`](AGENTS.md) for additional project conventions.

## Commit Messages

- Use the imperative mood: "Add foo" not "Added foo".
- Keep the subject line under 72 characters.
- A short body explaining the *why* is welcome for non-trivial changes.

## Reporting Bugs

Open a GitHub issue with a minimal reproduction. Include:

- `tome --version` output
- Operating system and shell
- The exact command you ran
- The expected vs. actual behavior

## Security Issues

See [`SECURITY.md`](SECURITY.md) — please do not file public issues for
security vulnerabilities.
