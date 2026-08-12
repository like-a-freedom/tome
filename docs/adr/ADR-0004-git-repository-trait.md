# ADR-0004: GitAdapter with GitRepository Trait

## Status
Deferred — revisit when a second Git backend or mock-driven service seam is required

## Context
`core/git.rs` (340 строк, 625 тестовых строк) — самый большой core-модуль. Содержит:
- Базовые операции: `get_commit_hash`, `get_branch`, `get_remote_url`
- Clone: `clone_repo`, `clone_repo_quiet`
- Sparse clone: `clone_repo_sparse` (~160 строк) — case-insensitive path search, sibling directory detection
- Helper: `has_path_in_tree`, `find_case_insensitive_path`, `find_sibling_dirs`

Это **shallow module** — интерфейс почти так же сложен, как реализация. Sparse clone logic (case mismatch, sibling dirs) — ~80 строк — следует вынести.

## Decision
Определить `GitRepository` trait с методами:
- `clone(url, dest, sparse) -> Result<PathBuf>`
- `commit_hash() -> Result<String>`
- `branch() -> Result<String>`
- `remote_url() -> Result<String>`

Реализации:
1. **DiskGitRepository** — обёртка над существующим `core/git` (использует `std::process::Command`)
2. **MockGitRepository** — для тестов (возвращает предопределённые результаты)

Sparse clone logic перемещается в `core/git_sparse.rs`.

## Consequences

### Положительные
- Основной git.rs сокращается с 340 до ~80 строк (базовые операции)
- Sparse clone edge cases (case mismatch, sibling dirs) изолированы в `git_sparse.rs`
- Тесты могут использовать MockGitRepository без спавна реального git
- Один адаптер = один гипотетический seam: если нужен remote git (SSH, HTTPS auth), интерфейс готов

### Отрицательные
- Добавляется `core/git_repository.rs` (~80 строк) + `core/git_sparse.rs` (~160 строк)
- Требуется рефакторинг всех вызовов `core::git::*`

### Совместимость с ADR-0001
- Deferred integration: current services call `core::git` directly because there is one concrete Git backend
- DESIGN.md §7 "Git CLI over git2" remains unchanged; a future DiskGitRepository would wrap core/git

## Files Affected
- `src/core/git.rs` (340 → ~80 lines)
- **NEW** `src/core/git_repository.rs` (~80 lines)
- **NEW** `src/core/git_sparse.rs` (~160 lines)
