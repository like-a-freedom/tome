# tome — Рефакторинг: План реализации

> Статус на 2026-07-26: Steps 1–2 реализованы в `src/services.rs`, `src/services/` и `src/core/spec.rs`. Современный layout намеренно заменяет запланированный ранее `src/services/mod.rs`. Git/filesystem/manifest adapters и domain model пока отложены: в проекте нет второго backend, а предложенные `impl Iterator` trait-интерфейсы не подходят для `dyn Trait` без дополнительного усложнения. `docs/superpowers/plans/` в текущем checkout пуст.

## Порядок выполнения (5 шагов)

Каждый шаг зависит от предыдущих. Выполнять последовательно:

```
Step 1 (2 дня) → Step 2 (0.5 дня) → Step 3 (1 день) → Step 4 (1 день) → Step 5 (2 дня)
```

---

## Step 1: Application Service Layer (ADR-0001)
**Зависит от:** ничего
**Время:** ~2 дня

### Что создать:
```
src/services.rs       — module declaration
src/services/
├── skill_service.rs   — SkillService install/remove orchestration
├── update_service.rs  — UpdateService update orchestration and reporter seam
├── adapters.rs        — deferred; add only with a second concrete backend
```

### Что изменить:
| Файл | До | После | Действие |
|------|-----|-------|----------|
| `commands/add.rs` (250 строк) | ~30 строк | Вызвать `SkillService::install()` |
| `commands/update.rs` (бывший `commands/update/mod.rs`) | тонкий адаптер | Вызвать `UpdateService::run()` |
| `commands/remove.rs` (50 строк) | ~15 строк | Вызвать `SkillService::remove()` |
| `commands/list.rs` (320 строк) | текущий read-side orchestration | Deferred: выделить read model только при появлении use case |
| `commands/export.rs` (40 строк) | текущий manifest read | Deferred: выделить read model только при появлении use case |
| `lib.rs` | добавить `pub mod services;` | — |

### Deferred interfaces (не реализованы в текущем service-layer шаге):

Следующие интерфейсы остаются roadmap proposal и не должны считаться wired functionality:

```rust
// src/services/skill_service.rs
pub trait SkillStore {
    fn is_tracked(&self, name: &str) -> bool;
    fn get_skill(&self, name: &str) -> Option<&SkillEntry>;
    fn all_skills(&self) -> impl Iterator<Item = (&str, &SkillEntry)>;
    fn skill_names(&self) -> Vec<String>;
}

pub struct SkillService<'a> {
    store: &'a dyn SkillStore,
    skills_dir: PathBuf,
}

// src/services/update_service.rs  
pub struct UpdateService<'a> {
    store: &'a dyn SkillStore,
    skills_dir: PathBuf,
}

// src/services/adapters.rs (trait для I/O)
pub trait GitAdapter {
    fn clone(&self, url: &str, dest: &Path, sparse: bool) -> Result<PathBuf>;
    fn commit_hash(&self, path: &Path) -> Result<String>;
    fn branch(&self, path: &Path) -> Result<String>;
    fn remote_url(&self, path: &Path) -> Result<String>;
}

pub trait FileSystemAdapter {
    fn copy_skill(&self, src: &Path, dest: &Path) -> Result<()>;
    fn ensure_skills_dir(&self, path: &Path) -> Result<()>;
}
```

### Deferred tests (будущий adapterized шаг):
- `tests/integration/skill_service_test.rs` — тесты с MemoryManifestStore + MockGitAdapter
- `tests/integration/update_service_test.rs` — тесты update с mock'ами

---

## Step 2: Single YAML Parser (ADR-0002) — Implemented
**Зависит от:** Step 1 (но можно делать параллельно — затрагивает другой модуль)
**Время:** ~0.5 дня

### Что изменить:
| Файл | До | После | Действие |
|------|-----|-------|----------|
| `core/spec.rs` (385 строк) | ~120 строк | Удалить `parse_frontmatter_fallback()` (~98 строк) |
| — | + preprocessing logic | Добавить `preprocess_frontmatter()` для экранирования описаний с `: ` |

### Ключевая логика preprocessing:
```rust
fn preprocess_frontmatter(yaml: &str) -> Result<String> {
    // Найти строки "description: ..." и если значение содержит ": ", экранировать его в кавычки
    // Остальные ключи (name, version и т.д.) — без изменений
}
```

### Тесты:
- Удалить все тесты `parse_frontmatter_fallback` (удаляются ~150 строк)
- Добавить тесты для `preprocess_frontmatter` с edge-case описаниями

---

## Step 3: ManifestStore Repository Pattern (ADR-0003)
**Зависит от:** Step 1 (использует trait из SkillStore)
**Время:** ~1 день

### Что создать:
```
src/core/manifest_store.rs  — pub trait ManifestStore (копия SkillStore)
                           — impl DiskManifestStore for Manifest  (обёртка)
                           — struct MemoryManifestStore { skills: HashMap<String, SkillEntry> }
```

### Что изменить:
| Файл | Действие |
|------|----------|
| `core/manifest.rs` | Добавить `pub trait ManifestStore` (из SkillStore) |
| `core/manifest_store.rs` | NEW — DiskManifestStore + MemoryManifestStore |
| `commands/export.rs` | Заменить `manifest.skills.iter()` на `store.all_skills()` |
| Step 1 (services) | Использовать `dyn ManifestStore` вместо прямых вызовов |

### Ключевой интерфейс:
```rust
pub trait ManifestStore {
    fn is_tracked(&self, name: &str) -> bool;
    fn get_skill(&self, name: &str) -> Option<&SkillEntry>;
    fn all_skills(&self) -> impl Iterator<Item = (&str, &SkillEntry)>;
    fn skill_names(&self) -> Vec<String>;
    fn save(&self, path: &Path) -> Result<()>;  // для DiskManifestStore
}

// MemoryManifestStore — for tests:
pub struct MemoryManifestStore {
    skills: HashMap<String, SkillEntry>,
}

// DiskManifestStore — wraps existing Manifest:
pub struct DiskManifestStore {
    manifest: Manifest,  // from core/manifest.rs
}

impl DiskManifestStore {
    pub fn load(path: &Path) -> Result<Self>;
}

impl ManifestStore for DiskManifestStore { ... }
```

---

## Step 4: GitRepository Trait (ADR-0004)
**Зависит от:** Step 1 (использует trait из GitAdapter)
**Время:** ~1 день

### Что создать:
```
src/core/git_repository.rs  — pub trait GitRepository { clone(), commit_hash()... }
                          — impl DiskGitRepository  (обёртка над core/git)
                          — struct MockGitRepository { for tests }

src/core/git_sparse.rs      — pub fn clone_sparse(...)  (переместить из core/git)
                            — helper functions: has_path_in_tree, find_case_insensitive_path, 
                              find_sibling_dirs
```

### Что изменить:
| Файл | До | После | Действие |
|------|-----|-------|----------|
| `core/git.rs` (340 строк) | ~80 строк | Базовые операции: `get_commit_hash`, `get_branch`, `get_remote_url`, `clone_repo` |
| — | +160 строк | Sparse clone logic → `core/git_sparse.rs` |
| — | +80 строк | GitRepository trait + DiskGitRepository → `core/git_repository.rs` |
| Step 1 (services) | Использовать `dyn GitRepository` | — |

### Ключевой интерфейс:
```rust
pub trait GitRepository {
    fn clone(&self, url: &str, dest: &Path, sparse: bool) -> Result<PathBuf>;
    fn commit_hash(&self, path: &Path) -> Result<String>;
    fn branch(&self, path: &Path) -> Result<String>;
    fn remote_url(&self, path: &Path) -> Result<String>;
}

// DiskGitRepository — wraps existing core/git functions:
pub struct DiskGitRepository;

impl GitRepository for DiskGitRepository {
    fn clone(&self, url: &str, dest: &Path, sparse: bool) -> Result<PathBuf> {
        if sparse {
            git::clone_repo_sparse(url, dest)  // из git_sparse.rs
        } else {
            git::clone_repo(url, dest)  // из core/git.rs (остаток ~80 строк)
        }
    }
}

// MockGitRepository — for tests:
pub struct MockGitRepository {
    pub expected_commit: String,
    pub expected_branch: String,
}

impl GitRepository for MockGitRepository { ... }  // возвращает предопределённые значения
```

---

## Step 5: Domain Model (ADR-0005)
**Зависит от:** Steps 1-4 (Service layer уже определён)
**Время:** ~2 дня

### Что создать:
```
src/domain/
├── skill.rs           — pub struct Skill { frontmatter: SkillFrontmatter }
├── resolved_skill.rs  — pub struct ResolvedSkill { skill: Skill, source: RepositoryRef }
├── repository_ref.rs  — pub struct RepositoryRef { label, remote_url, commit }
└── mod.rs             — pub use skill::Skill; resolved_skill::ResolvedSkill; ...
```

### Что изменить:
| Файл | Действие |
|------|----------|
| `core/spec.rs` → `domain/skill.rs` | SkillFrontmatter → Skill с методом `.validate()` |
| `core/install.rs` → `domain/resolved_skill.rs` | ResolvedSkill { 7 полей } → ResolvedSkill { skill: Skill, source: RepositoryRef } |
| `core/manifest.rs` → `domain/repository_ref.rs` | Вытащить label + remote_url + commit в RepositoryRef |
| Step 1 (services) | Использовать domain objects вместо raw structs |

### Ключевой интерфейс:
```rust
pub struct Skill {
    frontmatter: SkillFrontmatter,  // сохраняется для совместимости с существующими модулями
}

impl Skill {
    pub fn from_frontmatter(fm: SkillFrontmatter) -> Self;
    pub fn name(&self) -> &str { &self.frontmatter.name }
    pub fn description(&self) -> &str { &self.frontmatter.description }
    pub fn validate(&self) -> Result<()>;  // name length, required fields — из spec::validate_skill
}

pub struct RepositoryRef {
    label: String,
    remote_url: String,
    commit: String,  // "file:" for file URL skills
}

impl RepositoryRef {
    pub fn from_git(label, remote_url, commit) -> Self;
    pub fn from_file_url(url: &str, path: &str) -> Self;  // для file:// URL
}

pub struct ResolvedSkill {
    pub skill: Skill,
    pub source: RepositoryRef,  // вместо label + commit + remote по отдельности
}

// ResolvedSkill::from_detected() — из install.rs:
impl ResolvedSkill {
    pub fn from_detected(repo_path, temp_root, frontmatter, label, commit, remote, branch) -> Self {
        let skill = Skill::from_frontmatter(frontmatter);
        let source = RepositoryRef::from_git(label, remote, commit);
        ResolvedSkill { skill, source }  // path вычисляется из repo_path vs temp_root
    }
}
```

---

## Текущая структура после реализованного service-layer шага:

```text
src/
├── main.rs                  # thin CLI entry point
├── lib.rs                   # application entry point and module root
├── commands.rs + commands/  # thin mutation adapters; list/export read-side remain deferred
├── core.rs + core/          # domain-neutral parsing, manifest, git and filesystem logic
├── services.rs + services/  # application service layer
└── progress.rs              # presentation adapter for UpdateReporter
```

## Отложенная целевая структура проекта:

```
src/
├── main.rs                    # (не меняется)
├── lib.rs                     # +pub mod services, domain
├── config.rs                  # (не меняется)
├── fs.rs                      # (не меняется)
├── output.rs                  # (не меняется)
├── progress.rs                # (не меняется)
├── style.rs                   # (не меняется)
├── tui.rs                     # (не меняется)
├── testutil.rs                # (расширяется: MemoryManifestStore, MockGit)
├── commands/
│   ├── add.rs                 # ~30 строк (вызывает SkillService)
│   ├── remove.rs              # ~15 строк (вызывает SkillService)
│   ├── list.rs                # ~40 строк (использует ManifestStore)
│   ├── export.rs              # ~15 строк (использует ManifestStore)
│   └── update.rs              # тонкий адаптер, summary presentation helpers
├── core/
│   ├── spec.rs                # ~120 строк (только preprocessing + serde_yaml)
│   ├── git.rs                 # ~80 строк (базовые операции)
│   ├── git_sparse.rs          # NEW ~160 строк (sparse clone logic)
│   ├── git_repository.rs      # NEW ~80 строк (trait + DiskGitRepository)
│   ├── manifest.rs            # (не меняется, обёрнут DiskManifestStore)
│   ├── manifest_store.rs      # NEW ~100 строк (trait + Disk/ Memory impl)
│   ├── install.rs             # ~100 строк (обновлён для domain model)
│   ├── parse_source.rs        # (не меняется)
│   ├── repo_detect.rs         # (не меняется)
│   └── skip.rs                # (не меняется)
├── services.rs                # module declaration and re-exports
├── services/                  # application service layer
│   ├── skill_service.rs       # install/remove orchestration
│   └── update_service.rs      # update batch and reporter seam
└── domain.rs                  # deferred; add with domain/ only when needed
```

Эта схема остаётся deferred roadmap, а не текущим контрактом проекта. Новые adapters/repositories/domain types добавляются только при появлении реального use case и отдельного тестового seam.
