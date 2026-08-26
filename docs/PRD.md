# Tome PRD

## 1. Executive Summary

We're building a minimal CLI utility (`tome`) for managing AI agent skills in a centralized `.agents` directory, strictly compliant with the [agentskills.my specification v1.0.0](https://agentskills.my/specification/). The tool solves the problem of scattered skill files across multiple agent formats, resulting in a single source of truth for all installed skills with one-command install/update/delete.

---

## 2. Problem Statement

### Who has this problem?
Developers using AI coding agents (Claude Code, OpenCode, Cursor, Copilot, etc.) who install multiple skills from various sources.

### What is the problem?
Skills are scattered across multiple directories per the [platform compatibility table](https://agentskills.my/specification/#compatibility):

| Platform | Project Scope | Global Scope |
|----------|---------------|--------------|
| Claude Code | `.claude/skills/` | `~/.claude/skills/` |
| GitHub Copilot | `.github/skills/` | `~/.copilot/skills/` |
| Cursor | `.cursor/skills/` | `~/.cursor/skills/` |
| Gemini CLI | `.gemini/skills/` | `~/.gemini/skills/` |
| Windsurf | `.windsurf/skills/` | `~/.codeium/windsurf/skills/` |
| OpenAI Codex | `.codex/skills/` | `~/.codex/skills/` |
| OpenCode | `.opencode/skills/` | `~/.config/opencode/skills/` |

No unified way to manage them. Updates require visiting each source. Skills get duplicated. Formats differ between agents.

### Why is it painful?
- **Time waste:** Manually tracking 20+ skills across 4+ agents
- **Duplication:** Same skill installed in multiple locations
- **Drift:** Skills become inconsistent across agents
- **Update friction:** No built-in mechanism for updates

### Evidence
- 112 skills currently in `~/.agents/skills/`
- Multiple agent directories (`.opencode/`, `.claude/`, `.cursor/`, `.gemini/`)
- No centralized install/update mechanism

---

## 3. Target Users & Personas

### Primary Persona: Power User Paul
- **Role:** Developer using 2-4 AI agents
- **Tech savviness:** High (CLI-native)
- **Goals:** Keep skills organized, up-to-date, and accessible across agents
- **Pain points:** Manual skill management, duplication, version drift
- **Current behavior:** Copy/paste skills between directories, manual updates

### Secondary Persona: Agent Author Alice
- **Role:** Skill creator publishing to GitHub
- **Needs:** Distribute skills in standardized format
- **Differs from primary:** Focuses on creation, not consumption

---

## 4. Strategic Context

### Business Goals
- Reduce friction in skill adoption → more skills used per agent
- Standardize skill format → easier cross-agent compatibility
- Centralize management → reduce support burden

### Why Now?
- Agent ecosystem fragmented (46+ agents, 4+ formats)
- Skills marketplace growing (400K+ skills per SkillKit)
- No minimal CLI option exists (Simba/SkillKit are complex)
- [agentskills.my specification](https://agentskills.my/specification/) provides the standard

### Competitive Landscape

| Tool | Approach | Complexity | Spec Compliance |
|------|----------|------------|-----------------|
| **Simba** | Central store + symlinks, multi-agent sync, TUI | High | Partial |
| **SkillKit** | Marketplace + translation + mesh + memory | Very High | Partial |
| **skills.sh** | Leaderboard + `npx skills add` + telemetry | Low | Full |
| **This tool** | Minimal CLI, single `.agents` directory | **Low** | **Full** |

---

## 5. Solution Overview

We're building a **minimal CLI** that manages skills in a single `.agents/` directory, fully compliant with the [agentskills.my specification](https://agentskills.my/specification/).

### Core Commands

```bash
# Install a skill from GitHub
tome add anthropics/skills

# Install specific skill from multi-skill repo
tome add anthropics/skills@skill-name

# Update all installed skills
tome update

# Update specific skill
tome update skill-name

# Remove a skill
tome remove skill-name

# List installed skills
tome list
```

### Command Flags

| Command | Flag | Description |
|---------|------|-------------|
| `add` | `--path <path>` | Skill path within repo (if not root) |
| `add` | `--force` | Overwrite if already installed |
| `add` | `--no-sparse` | Download full repo (disable sparse clone) |
| `update` | `<name>` | Update only the named skill (optional) |
| `update` | `--verbose` | Show full update details |
| `update` | `--no-sparse` | Download full repo (disable sparse clone) |
| `remove` | `--force` | Skip confirmation prompt |
| `list` | `--json` | Output as JSON |
| all | `--dir <path>` | Override skills directory (or `SKILLS_DIR` env) |
| `export` | — | Print `tome add` commands for all tracked skills |

### Skill Directory Structure (per spec)

```
skill-name/
├── SKILL.md            # REQUIRED
├── scripts/
│   └── *.sh / *.py     # Optional executable scripts
├── references/
│   └── *.md / *.txt    # Optional reference documents
└── assets/
    └── *.png / *.svg   # Optional images, diagrams
```

### SKILL.md Format (per spec)

```markdown
---
name: Systematic Debugging
description: Step-by-step debugging workflow for any bug or test failure
version: 1.0.0
authors:
  - obra
tags: [debugging, testing, workflow]
platforms: [claude-code, opencode, cursor]
license: MIT
---

# Systematic Debugging

Use this skill when encountering any bug, test failure, or unexpected behavior.

## Steps

1. **Reproduce** — Create a minimal reproduction case
2. **Hypothesize** — Form hypotheses about the root cause
3. **Test** — Verify each hypothesis with evidence
4. **Fix** — Implement the minimal fix
5. **Verify** — Confirm the fix works and doesn't break other tests
```

#### YAML Frontmatter Fields

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `name` | ✅ Yes | string | Human-readable skill name (<50 chars) |
| `description` | ✅ Yes | string | One-line summary (~100 tokens budget) |
| `version` | ⚠️ Rec. | semver | Semantic version for update tracking |
| `authors` | ⚠️ Rec. | string[] | GitHub usernames or org names |
| `tags` | ⚠️ Rec. | string[] | Searchable keywords |
| `platforms` | ❌ Opt. | string[] | Compatible platform slugs (omit = all) |
| `license` | ❌ Opt. | string | SPDX license identifier |
| `repository` | ❌ Opt. | url | Source repository URL |
| `docs` | ❌ Opt. | url | Extended documentation link |

### Progressive Loading Model (per spec)

The spec defines how **agents** load skills at runtime. Our CLI installs files that agents discover:

1. **Discovery (Agent startup):** Agent scans `.agents/skills/`, loads ONLY YAML frontmatter from each SKILL.md (~100 tokens/skill)
2. **Activation (On demand):** Agent loads full SKILL.md when skill is referenced
3. **Resources (Lazy):** Agent loads scripts/, references/, assets/ only when needed

**The CLI's role:** Install spec-compliant files so agents can follow this model.

### File Structure

```
.agents/
├── skills/
│   ├── brainstorming/
│   │   ├── SKILL.md
│   │   └── scripts/
│   ├── tdd/
│   │   ├── SKILL.md
│   │   └── references/
│   └── .skills.json          # Install manifest
└── AGENTS.md
```

### Install Manifest (`.skills.json`)

```json
{
  "version": 1,
  "skills": {
    "brainstorming": {
      "source": "obra/superpowers",
      "path": "skills/brainstorming",
      "installed": "2026-06-18T10:00:00Z",
      "updated": "2026-06-18T10:00:00Z",
      "version": "1.0.0",
      "description": "Turn ideas into designs through collaborative dialogue",
      "commit": "a1b2c3d",
      "branch": "main",
      "remote": "https://github.com/obra/superpowers.git"
    },
    "tdd": {
      "source": "obra/superpowers",
      "path": "skills/tdd",
      "installed": "2026-06-18T10:00:00Z",
      "updated": "2026-06-18T12:00:00Z",
      "version": "1.0.0",
      "description": "Test-driven development workflow",
      "commit": "e4f5g6h",
      "branch": "main",
      "remote": "https://github.com/obra/superpowers.git"
    }
  }
}
```

#### Manifest Fields

| Field | Type | Description |
|-------|------|-------------|
| `source` | string | GitHub `owner/repo` |
| `path` | string | Skill path within repo |
| `installed` | ISO 8601 | First install timestamp |
| `updated` | ISO 8601 | Last update timestamp |
| `version` | semver | From SKILL.md frontmatter |
| `description` | string | From SKILL.md frontmatter |
| `commit` | string | Git commit hash (short) |
| `branch` | string | Git branch (default: "main") |
| `remote` | string | Git remote URL |

---

## 6. Success Metrics

### Primary Metric
**Skill install success rate:** 100% of `tome add` commands result in spec-compliant SKILL.md files

### Secondary Metrics
- **Install time:** < 5 seconds per skill
- **Update time:** < 10 seconds for all skills
- **Zero duplication:** Each skill exists in exactly one location
- **Spec compliance:** All installed skills pass YAML frontmatter validation

### Guardrail Metrics
- **No agent breakage:** Skills remain accessible to agents after operations
- **No data loss:** Existing skills preserved during updates
- **No telemetry:** Zero data collection (unlike skills.sh)

---

## 7. User Stories & Requirements

### Epic Hypothesis
We believe that a minimal CLI managing skills in a centralized `.agents` directory will reduce skill management friction by 80% because users currently waste time on manual duplication and version tracking.

### User Stories

**Story 1: Install a skill from GitHub**
As a developer, I want to install a skill from a GitHub repo so I can use it in my agent.

**Acceptance Criteria:**
- [ ] `tome add owner/repo` clones the repo (shallow clone, depth=1)
- [ ] Skill files are copied to `.agents/skills/<skill-name>/`
- [ ] `SKILL.md` is required at root or specified via `--path`
- [ ] YAML frontmatter is validated (name, description required)
- [ ] `.skills.json` manifest is updated with source, version, description
- [ ] Temp directory is cleaned up on success
- [ ] Error if no SKILL.md found

**Story 2: Install specific skill from multi-skill repo**
As a developer, I want to install a specific skill from a repo containing multiple skills.

**Acceptance Criteria:**
- [ ] `tome add owner/repo@skill-name` installs only that skill
- [ ] Path is validated (SKILL.md must exist)
- [ ] Manifest tracks the specific path

**Story 3: Update all skills**
As a developer, I want to update all installed skills to latest versions.

**Acceptance Criteria:**
- [ ] `tome update` fetches latest from each source
- [ ] Only changed files are updated
- [ ] New skills that appeared in a tracked repo are installed automatically (pinned to the source commit)
- [ ] Manifest versions are updated
- [ ] Temp directory is cleaned up on success/failure

**Story 4: Remove a skill**
As a developer, I want to remove a skill I no longer need.

**Acceptance Criteria:**
- [ ] `tome remove skill-name` deletes the skill directory
- [ ] Manifest is updated
- [ ] Confirmation prompt (skip with `--force`)

**Story 5: List installed skills**
As a developer, I want to see all installed skills.

**Acceptance Criteria:**
- [ ] `tome list` shows skill names, versions, and sources
- [ ] Output is table or simple list
- [ ] `--json` flag outputs as JSON

**Story 6: Run on existing .agents directory with manually-placed skills**
As a developer, I want to start using `tome` on a project that already has a `.agents/skills/` directory with skills I added manually — without breaking anything.

**Acceptance Criteria:**
- [ ] `tome list` scans `.agents/skills/` and shows ALL skills (tracked + untracked)
- [ ] Untracked skills are marked with `?` source (not installed by tome)
- [ ] `tome add` does NOT overwrite untracked skills (error, requires `--force`)
- [ ] `tome update` only updates skills in the manifest (skips untracked)
- [ ] `tome remove` only removes tracked skills (rejects untracked)
- [ ] `tome list --json` includes `"tracked": true/false` field
- [ ] No data loss on any operation

**Story 7: Rebuild manifest from existing skills**
As a developer, I want `tome` to detect when the manifest is missing or incomplete and rebuild it from the skills directory.

**Acceptance Criteria:**
- [ ] If `.skills.json` is missing, `tome list` rebuilds it by scanning `.agents/skills/`
- [ ] Rebuilt manifest marks all skills as `source: "unknown"` (untracked)
- [ ] `tome update` with no manifest runs against an empty manifest (reports "No skills to update"); `tome list` rebuilds the manifest
- [ ] No skills are deleted during rebuild

### Constraints
- SKILL.md format is the only supported format (per spec)
- GitHub is the primary remote source (for v1); `file://` URLs are supported for local files and repositories
- No network operations beyond Git clone/fetch or explicit file-URL downloads
- No configuration beyond `.skills.json`
- No telemetry or data collection
- No format translation (each agent reads SKILL.md directly)
- Git must be installed on the system

### Edge Cases
- What if `.agents/skills/` doesn't exist? → Create it
- What if `.agents/skills/` exists with skills but no manifest? → Rebuild manifest from directory scan
- What if `owner/repo` is a multi-skill repo (no `@skill-name`)? → Show list, prompt user to select
- What if SKILL.md is in subdirectory? → Auto-detected; multi-skill repos prompt for interactive selection (`--path` narrows to one skill)
- What if skill already installed? → Error message, use `--force` to overwrite
- What if skill exists but isn't in manifest (manually placed)? → Error message, use `--force` to overwrite
- What if untracked skill has same name as new install? → Error: conflict, use `--force` to overwrite
- What if git is not installed? → Clear error message with install instructions
- What if YAML frontmatter is invalid? → Error with validation details
- What if skill has no version? → Use "0.0.0" as default
- What if clone fails midway? → Clean up temp directory, return error
- What if manifest is corrupted? → Rebuild from installed directories
- What if manifest references deleted skill? → Report as stale (preserve the entry, warn user); no data loss
- What if skills directory has non-skill files (README, etc.)? → Ignore, only process SKILL.md directories

---

## 8. Out of Scope

**Not included in this release:**
- **Format translation** (SKILL.md → .mdc, etc.) — Keep simple, one format only
- **Multi-agent sync** (symlinks to .claude/, .opencode/) — Users can configure agents to read from .agents/
- **Marketplace/search** — Focus on install from explicit GitHub sources
- **TUI/interactive mode** — CLI only
- **Backup/snapshot** — Simple deletion, git history is backup
- **Session memory** — Out of scope
- **MCP/REST API** — CLI only
- **Telemetry** — Zero data collection (unlike skills.sh)
- **Skill ranking/leaderboard** — Out of scope

**Future consideration:**
- `tome init` to scaffold `.agents/` structure
- `tome search` for marketplace discovery
- `tome sync` to update agent-specific directories
- `tome validate` to lint SKILL.md files against spec

---

## 9. Dependencies & Risks

### Dependencies
- **Git:** Must be installed for cloning repos
- **Network:** Internet access for GitHub
- **SKILL.md format:** Users must provide valid skill files per [spec](https://agentskills.my/specification/)

### Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| GitHub rate limiting | Cache clones, use local git operations |
| Corrupted skill files | Validate SKILL.md exists before install |
| Invalid YAML frontmatter | Error with clear validation message |
| Name collisions | Use repo/skill-name as unique identifier |
| Agent format changes | Sticking to SKILL.md (standard per spec) |
| Partial clone failure | Clean up temp directory on any failure |
| Corrupted manifest | Rebuild from installed directories |

---

## 10. Open Questions

- Should we support private repos with SSH? (Decision: Yes, via git credentials)
- Should we support non-GitHub sources (GitLab, local)? (Decision: No, v1 GitHub only)
- Should `tome update` create backups? (Decision: No, rely on git history)
- Should we validate skill quality (lint SKILL.md)? (Decision: No, out of scope)
- Should we add a `--validate` flag to check spec compliance? (Decision: Yes, v1.1)
- Should we support `skills.sh` leaderboard integration? (Decision: No, separate tool)

---

## 11. Installation

### How users install the CLI

```bash
# macOS / Linux
curl -sSfL https://github.com/solovey/tome/releases/latest/download/tome-$(uname -s)-$(uname -m).tar.gz | tar xz -C /usr/local/bin

# Windows (PowerShell)
Invoke-WebRequest -Uri https://github.com/solovey/tome/releases/latest/download/tome-x86_64-pc-windows-msvc.zip -OutFile tome.zip
Expand-Archive tome.zip -DestinationPath $env:LOCALAPPDATA\Microsoft\WinGet\Links

# From source
git clone https://github.com/solovey/tome
cd tome
cargo install --path .
```

---

## References

- **Agent Skills Specification:** https://agentskills.my/specification/ — The open standard
- **skills.sh:** https://www.skills.sh/ — Leaderboard + `npx skills add`
- **Simba:** https://github.com/ethanfann/simba — Central store, symlinks, registry tracking
- **SkillKit:** https://github.com/rohitg00/skillkit — Format translation, marketplace, 46+ agents
- **Superpowers:** https://github.com/obra/superpowers — Reference skill collection
