# Domain Glossary — tome

## Core Concepts

**Skill** — An AI agent instruction set packaged as a `SKILL.md` file with YAML frontmatter (name, description, version, tags, etc.). Skills are the atomic unit of tome.

**Manifest** — The `.skills.json` file that records all installed skills, their source repos, commit hashes, and installation metadata. Single source of truth for what's installed.

**SkillEntry** — A single record in the manifest: tracks one installed skill's source, commit, remote URL, branch, path, version, and timestamps.

**ResolvedSkill** — A skill that has been located in a repository and validated, ready for installation. Contains repo path, frontmatter, label, commit hash, remote URL, and branch.

**InstallResult** — The outcome of an install operation: lists of installed, skipped-tracked, and skipped-existing skill names.

## Sources

**Source** — A parsed input to `tome add`: can be `owner/repo`, `owner/repo@skill-name`, or a `file:///` URL pointing to a SKILL.md file.

**File URL** — A `file:///path/to/SKILL.md` source that downloads a single SKILL.md file via curl rather than cloning a repository.

## Repositories

**RepoType** — Classification of a cloned repository: `SingleSkillAtRoot` (one SKILL.md at root), `SingleSkillInDir` (one skill in a subdirectory), `MultiSkill` (multiple skills in subdirectories).

**SkillInfo** — A detected skill within a repository: name, path relative to repo root, optional description.

## Operations

**Sparse Clone** — A git clone that checks out only skill-relevant directories (`skills/`, `Skills/`, `.claude/skills/`, `.agents/skills/`) plus sibling directories, reducing download size.

**Stale Entry** — A manifest entry whose skill directory no longer exists on disk. Preserved in the manifest (not deleted) to avoid data loss.

**Untracked Skill** — A skill directory on disk that exists in the skills directory but is not tracked in the manifest.

## Paths

**Skills Directory** — The local directory where skills are installed (default: `~/.agents/skills/`). Configurable via `--dir` or `SKILLS_DIR` env var.

**SKILL_SUBDIRS** — The set of directory names within a repository that may contain skills: `["skills", "Skills", ".claude/skills", ".agents/skills"]`.

**SKIP_DIRS** — Directories to skip when scanning for skills: `.git`, `node_modules`, `.github`, etc.

## Services

**SkillService** — Application service managing skill lifecycle: install (from git or file URL) and remove.

**UpdateService** — Application service managing skill updates: stale detection, git-group updates, file-skill updates, and installing new skills that appear in tracked repositories (from an untargeted update).

**UpdateReporter** — Trait for reporting update progress without coupling the service to a terminal implementation.
