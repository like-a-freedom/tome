# ADR-0004: GitAdapter with GitRepository Trait

## Status
Partially realized — sparse-clone logic was extracted to `core/git/git_sparse.rs`; the `GitRepository` trait remains deferred until a second Git backend or mock-driven service seam is required

## Context
`core/git.rs` (307 строк) + `core/git/git_sparse.rs` (706 строк с тестами) — модуль разбит на два файла. Содержит:
- Базовые операции: `get_commit_hash`, `get_branch`, `get_remote_url`
- Clone: `clone_repo`, `clone_repo_quiet`, `clone_repo_or_sparse`
- Sparse clone: `git_sparse::clone_repo_sparse` — case-insensitive path search, sibling directory detection
- Helper: `has_path_in_tree`, `find_case_insensitive_path`, `find_sibling_dirs` (в `git/git_sparse.rs`)

Это **shallow module** — интерфейс почти так же сложен, как реализация.

## Decision
Определить `GitRepository` trait с методами:
- `clone(url, dest, sparse) -> Result<PathBuf>`
- `commit_hash() -> Result<String>`
- `branch() -> Result<String>`
- `remote_url() -> Result<String>`

Реализации:
1. **DiskGitRepository** — обёртка над существующим `core/git` (использует `std::process::Command`)
2. **MockGitRepository** — для тестов (возвращает предопределённые результаты)

Sparse clone logic уже вынесена в `core/git/git_sparse.rs` (реализовано); оставшаяся часть решения (trait + обёртки) — deferred.

## Consequences

### Положительные
- Основной git.rs сокращается до базовых операций
- Sparse clone edge cases (case mismatch, sibling dirs) изолированы в `core/git/git_sparse.rs`
- Тесты могут использовать MockGitRepository без спавна реального git
- Один адаптер = один гипотетический seam: если нужен remote git (SSH, HTTPS auth), интерфейс готов

### Отрицательные
- Добавляется `core/git/git_sparse.rs` (уже реализовано) + deferred `core/git_repository.rs`
- Требуется рефакторинг всех вызовов `core::git::*`

### Совместимость с ADR-0001
- Deferred integration: current services call `core::git` directly because there is one concrete Git backend
- DESIGN.md §7 "Git CLI over git2" remains unchanged; a future DiskGitRepository would wrap core/git

## Files Affected
- `src/core/git.rs` (340 → ~110 lines, базовые операции + диспетчер sparse)
- **DONE** `src/core/git/git_sparse.rs` (sparse clone, case/sibling helpers)
- **DEFERRED** `src/core/git_repository.rs` (~80 lines)
