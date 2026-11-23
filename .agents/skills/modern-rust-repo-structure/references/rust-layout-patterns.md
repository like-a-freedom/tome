# Rust layout patterns

Single-crate, multi-binary, and workspace layouts, feature flags, and profiles.
Load this file for projects beyond tome's single crate.

## Simple crate (CLI / utility)

```text
my-tool/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs        ← entry point only, argument parsing
│   ├── lib.rs         ← pub mod ...; public API root
│   ├── config.rs
│   ├── error.rs       ← single error type (thiserror / anyhow)
│   └── utils/
│       ├── parser.rs
│       └── formatter.rs
├── tests/
│   └── integration.rs
└── examples/
    └── basic.rs
```

- `main.rs`: read args, build `Config`, call `run(config)`, print errors.
- `lib.rs`: export `run`, modules, and public types.
- `config.rs`: argument shape and parsing (`clap`, `argh`).
- `error.rs`: one error enum, converted to `anyhow::Error` or `miette::Report`.
- `examples/`: user scenarios without duplicating core logic.

## Library plus multiple binaries

```text
src/
├── lib.rs
└── bin/
    ├── server.rs   ← cargo run --bin server
    └── client.rs
```

- Shared types, errors, and functions live in `lib.rs`.
- `bin/*.rs` hold entry points only.
- Run with `cargo run --bin <name>`; build all with `cargo build --bins`.

## Virtual workspace

```toml
# Cargo.toml
[workspace]
members = ["crates/*"]
resolver = "2"
```

```text
crates/
├── core/    ← shared types, traits, errors
├── server/
├── cli/
└── proto/
```

- One `Cargo.lock` and shared `target/`; `cli` and `server` depend on `core`,
  not on each other.
- `proto` holds generated Protobuf/gRPC types and codegen artifacts.
- Check with `cargo check --workspace`, `cargo test --workspace`,
  `cargo build --all-features`.

## Feature flags

```toml
[features]
default = []
async   = ["dep:tokio"]
tls     = ["dep:rustls", "async"]
full    = ["async", "tls"]
```

- Each feature adds behavior; none disables other code.
- `default = []` keeps library dependencies explicit.
- CI: `cargo hack check --feature-powerset` or `cargo test --all-features`;
  per crate: `cargo test -p <crate> --all-features`.

## Build profiles (workspace root only)

```toml
[profile.dev.package.serde]
opt-level = 3

[profile.release]
lto = "thin"
codegen-units = 1
strip = true
```

Declare profiles once, in the workspace root `Cargo.toml`; per-crate copies
drift and break maintenance.

## `mod.rs` vs 2018+ style

| Style | File |
|-------|------|
| Old | `utils/mod.rs` |
| New (2018+) | `utils.rs` + folder `utils/` |

Flat modules navigate and refactor easier; prefer them in Rust 2018+.

## Reference repositories

- `ripgrep` — multi-crate layout, feature flags, performance.
- `tokio` — virtual workspace, separate `core` and `util`.
- `axum` — `axum-core` / `axum-extra` split.
- `clap` — tidy feature gates and re-exports.
- `serde` — separate proc-macro crate.
