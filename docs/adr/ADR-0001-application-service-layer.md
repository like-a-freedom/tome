# ADR-0001: Application Service Layer Pattern

## Status
Accepted — service extraction implemented; I/O adapter traits remain deferred under YAGNI

## Context
tome — тонкий CLI для управления AI agent skills. Бизнес-логика (резолюция skill'ов, определение типа репозитория, клонирование, валидация SKILL.md) размазана по command модулям (`commands/add.rs`, `commands/update/mod.rs`, `commands/list.rs`). Каждая команда сама парсит источник, клонирует репо, определяет тип репозитория, валидирует skill и обновляет манифест.

## Decision
Внедрить Application Service Layer с двумя сервисами:

1. **SkillService** — управляет жизненным циклом skill (install, remove, resolve)
2. **UpdateService** — управляет обновлением (stale detection, git-группы, file-skill)

Mutation-команды становятся thin-адаптерами: `CLI args → service call → output formatting`.
Read-side orchestration в `list` и `export` пока остаётся в command modules и deferred.

Текущая реализация делегирует I/O существующим core-модулям напрямую. Trait-адаптеры
`GitAdapter`, `FileSystemAdapter` и in-memory manifest store остаются deferred до
появления второго backend или отдельного mock-driven seam.

## Consequences

### Положительные
- Бизнес-логика изолирована в одном месте — `services/skill_service.rs`, `services/update_service.rs`
- Add/remove/update-команды становятся тонкими; list/export read-side extraction остаётся deferred
- Тестируемость: orchestration покрыта unit/integration тестами через локальные file:// repositories
- `UpdateReporter` предоставляет реальный seam для progress/presentation без привязки service к terminal implementation

### Отрицательные
- Добавляется новый модуль `services/`
- Полная adapterization остаётся отдельным изменением и не навязывается маленькому single-backend CLI

### Неизменные решения из DESIGN.md
- "Absolute minimalism" — сервисы не добавляют бэкенд, только структурируют существующую логику
- "File-Based State" — Persistency остаётся файловой; `ManifestStore` trait — deferred
- "Git CLI over git2" — используется существующий core/git; `GitAdapter` — deferred

## Files Affected
- `src/commands/add.rs` (250 → ~30 lines)
- `src/commands/update.rs` (former `src/commands/update/mod.rs`)
- `src/commands/remove.rs`
- **DEFERRED** `src/commands/list.rs` read-side extraction
- **NEW** `src/services.rs` (modern module declaration)
- **NEW** `src/services/skill_service.rs`
- **NEW** `src/services/update_service.rs`
