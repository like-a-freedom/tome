# ADR-0005: Domain Model for Skill and Manifest

## Status
Deferred — revisit when the domain model gains behavior beyond current core structs

## Context
Единственное domain-понятие в tome — **skill**, но оно представлено как plain data struct `SkillFrontmatter` без поведения. Нет domain model для отношений между skill'ами, манифестом и репозиториями.

Сейчас:
- `SkillFrontmatter` — data-only struct (name, description, version...)
- `ResolvedSkill` — "God Object" с 7 полями (repo_path, frontmatter, label, path, commit, remote, branch)
- `InstallResult` — data-only struct с 3 Vec<String>

Каждая команда переимплементирует логику резолвации skill'ов (resolve_names, is_tracked, stale detection).

## Decision
Внедрить domain objects с поведением:

```rust
pub struct Skill {
    frontmatter: SkillFrontmatter,
}

impl Skill {
    pub fn name(&self) -> &str;
    pub fn description(&self) -> &str;
    pub fn validate(&self) -> Result<()>;  // name length, required fields
}

pub struct ResolvedSkill {
    skill: Skill,
    source: RepositoryRef,  // label + remote URL + commit
}

pub struct InstalledSkill {
    skill: Skill,
    entry: ManifestEntry,  // source info for manifest
}

pub struct RepositoryRef {
    label: String,
    remote_url: String,
    commit: String,
}

impl RepositoryRef {
    pub fn from_git(url) -> Self;
    pub fn from_file_url(url, path) -> Self;
}
```

## Consequences

### Положительные
- **DDD**: Skill, Manifest, Repository — first-class domain concepts с поведением
- **Locality**: правила валидации живут на Skill, не размазаны по командам
- **Navigability**: будущие разработчики видят domain story: Skill → Manifest → Repository
- **Testability**: можно тестировать валидацию без git/fs

### Отрицательные
- Добавляет абстракционный слой (overhead для маленького проекта)
- Требует рефакторинга всех мест, где `SkillFrontmatter` используется напрямую

### Совместимость с ADR-0001
- Зависит от: Service layer использует domain objects вместо raw structs
- Не конфликтует с "Absolute minimalism" — domain model тонкий (только валидация и резолвация)

### Совместимость с DESIGN.md
- Не противоречит: domain model не добавёт бэкенд, только структурирует существующие data structs
- "One directory, one format" — domain model отражает эти принципы

## Files Affected
- `src/core/spec.rs` → `domain/skill.rs` (~80 lines)
- `src/core/install.rs` → `domain/resolved_skill.rs` (~60 lines)
- `src/core/manifest.rs` → `domain/repository_ref.rs` (~40 lines)
