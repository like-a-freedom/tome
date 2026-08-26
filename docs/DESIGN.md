# Tome Design Decisions

## Core Philosophy

**Absolute minimalism.** One directory, one format, one source (GitHub). Fully compliant with [agentskills.my specification v1.0.0](https://agentskills.my/specification/).

## Key Decisions

### 1. Single Directory: `.agents/skills/`

**Decision:** All skills live in `.agents/skills/`, no symlinks to other directories.

**Why:**
- Single source of truth
- No sync issues
- Agents can read from this directory directly
- Simpler than Simba's symlink approach

**Trade-off:** Users must configure their agents to read from `.agents/skills/`

### 2. SKILL.md Format Only (per spec)

**Decision:** Only support [SKILL.md format](https://agentskills.my/specification/#skill-md), no translation.

**Why:**
- SKILL.md is the [emerging standard](https://agentskills.my/specification/)
- Translation adds massive complexity (46+ formats)
- Keeps tool minimal

**Trade-off:** Users with non-SKILL.md agents need separate tools

### 3. GitHub as Sole Source (+ file://)

**Decision:** v1 supports GitHub repos and `file:///path/to/local/repo` URLs.

**Why:**
- Most skills are on GitHub
- Git clone is simple and reliable
- No API keys needed for public repos
- `file://` enables local development and testing without push

**Trade-off:** No GitLab, Bitbucket, or other hosting (v2+)

### 4. File-Based State (`.skills.json`)

**Decision:** Store install state in `.agents/skills/.skills.json`

**Why:**
- No database or server needed
- Easy to inspect/edit manually
- Git-trackable

**Trade-off:** No atomic operations (concurrent installs could conflict)

### 5. No Agent Sync

**Decision:** Don't symlink to `.claude/`, `.opencode/`, etc.

**Why:**
- Agents should read from `.agents/skills/` directly
- Configuring agents is simpler than maintaining symlinks
- Avoids Simba's complexity

**Trade-off:** Users must update agent configs manually

### 6. Zero Telemetry

**Decision:** No data collection whatsoever.

**Why:**
- Privacy-first approach
- No server infrastructure needed
- Unlike skills.sh which collects telemetry

**Trade-off:** No leaderboard or ranking integration

### 7. Git CLI over git2

**Decision:** Use `std::process::Command` to call git CLI instead of `git2` crate.

**Why:**
- No native libgit2 dependency (smaller binary, no build-time linking)
- Git is already installed on developer machines
- Simpler implementation
- No version compatibility issues

**Trade-off:** Requires git to be installed (but it's universal among target users)

## Comparison with References

| Aspect | Simba | SkillKit | skills.sh | This Tool |
|--------|-------|----------|-----------|-----------|
| Storage | Central + symlinks | Per-agent | Per-agent | Single `.agents/` |
| Formats | SKILL.md | 46+ formats | SKILL.md | SKILL.md only |
| Registry | JSON + config | Server + DB | Leaderboard | `.skills.json` |
| Telemetry | Unknown | Unknown | Yes | **No** |
| Complexity | High | Very High | Low | **Low** |
| Spec compliance | Partial | Partial | Full | **Full** |
| Native deps | Bun | Node.js | Node.js | **None** (git only) |

## Spec Compliance

Per [agentskills.my specification](https://agentskills.my/specification/):

The spec defines how **agents** load skills at runtime. The CLI's role is to install spec-compliant files so agents can follow the progressive loading model:

1. **Discovery:** Agent loads YAML frontmatter (~100 tokens/skill) → CLI ensures SKILL.md has valid frontmatter
2. **Activation:** Agent loads full SKILL.md on demand → CLI ensures file is accessible
3. **Resources:** Agent loads scripts/references/assets lazily → CLI copies entire skill directory

### What the CLI validates:
- ✅ SKILL.md exists
- ✅ YAML frontmatter has required fields (`name`, `description`)
- ✅ Optional fields are valid types
- ✅ Directory structure is spec-compliant

### What the CLI does NOT do:
- ❌ Load skills at startup (agents do this)
- ❌ Parse full SKILL.md content (agents do this)
- ❌ Execute scripts or load assets (agents do this)

## Failure Modes

| Failure | Behavior |
|---------|----------|
| Clone fails | Clean up temp dir, return error |
| SKILL.md missing | Error with clear message |
| Invalid YAML frontmatter | Error with validation details |
| Manifest corrupted | Rebuild from installed directories |
| Skill already installed | Require `--force` flag |
| Git not installed | Error with install instructions |

## Future Considerations (v2+)

- `tome init` — Scaffold `.agents/` structure
- `tome search` — Marketplace discovery
- `tome sync` — Update agent directories
- `tome validate` — Lint SKILL.md files against spec
- `tome pack` — Create distributable skill bundles
- `tome publish` — Submit to agentskills.my directory

## Implemented Features

- `tome export` — Outputs `tome add` commands for all tracked skills
- `file:///` URL support — Install skills from local repos
- `Skills/` directory support — Detects `Skills/<agent>/<skill-name>/SKILL.md` with deduplication across agents
- `.agents/skills/` bundle support — Sparse checkout fetches a nested `.agents/skills/` when present
- Top-level skill directories — Detects `skill-name/SKILL.md` at repo root
- New-skill sync on update — `tome update` installs skills that newly appear in tracked repos, pinned to the source commit
