# ADR-0003: ManifestStore Repository Pattern

## Status
Deferred — revisit when a second manifest backend or in-memory repository seam is required

## Context
`core::manifest` раскрывает внутреннюю структуру `HashMap<String, SkillEntry>` напрямую. Команды обращаются к `manifest.skills.iter()` (export.rs), что нарушает инкапсуляцию. Любой модуль, которому нужны данные манифеста, должен знать о внутреннем устройстве HashMap.

## Decision
Определить `ManifestStore` trait с методами:
- `is_tracked(name) -> bool`
- `get_skill(name) -> Option<&SkillEntry>`
- `all_skills() -> impl Iterator<Item = (&str, &SkillEntry)>`
- `skill_names() -> Vec<String>`
- `save(path) -> Result<()>`

Реализации:
1. **DiskManifestStore** — обёртка над существующим `core::manifest::Manifest` (сохраняет всю текущую функциональность)
2. **MemoryManifestStore** — in-memory HashMap для тестов (без создания temp-файлов)

## Consequences

### Положительные
- Тесты могут использовать `MemoryManifestStore` без создания temp файлов — быстрее и надёжнее
- Инкапсуляция: команды не знают о внутреннем устройстве HashMap
- Один адаптер = один гипотетический seam: если нужен remote manifest (cloud-synced), интерфейс готов

### Отрицательные
- Добавляется новый модуль `core/manifest_store.rs` (~100 строк)
- Требуется рефакторинг всех мест, где `manifest.skills` используется напрямую (export.rs)

### Совместимость с ADR-0001
- Deferred integration: current services use `Manifest` directly because there is one file-backed implementation
- If accepted later, `DiskManifestStore` should wrap the existing `Manifest` without changing the public CLI API

## Files Affected
- `src/core/manifest.rs` (добавляется `pub trait ManifestStore`)
- **NEW** `src/core/manifest_store.rs` (~100 lines)
- `src/commands/export.rs` (заменить `manifest.skills.iter()` на `store.all_skills()`)
