# Agent Skills

This document describes the skills system that provides specialized instructions to the AI agent on demand.

## Overview

Skills are self-contained instruction modules stored as `SKILL.md` files. They allow the agent to access domain-specific knowledge (debugging procedures, evaluation criteria, workflow instructions) only when needed, rather than embedding all possible knowledge in the initial prompt.

The system has three phases:

1. **Discovery**: Find `SKILL.md` files in `.agents/skills/` directories
2. **Catalog disclosure**: List discovered skills in an initial developer context message
3. **Activation**: The model reads a `SKILL.md` via the `Read` tool when its description matches the task

## Skill Format

Each skill is a directory containing a `SKILL.md` file:

```
.agents/skills/
  debugging-cake/
    SKILL.md
  evaluating-cake/
    SKILL.md
```

### SKILL.md Format

A `SKILL.md` file has YAML frontmatter followed by markdown body content:

```yaml
---
name: debugging-cake
description: |
  How to investigate and debug issues with the cake CLI tool. Use this skill whenever:
  - The user reports the CLI returned "None" or an empty response
  - The user mentions truncated, incomplete, or cut-off responses
---

# Debugging cake CLI

## Step 1: Check the session file
...
```

### Required Frontmatter Fields

| Field | Description |
|-------|-------------|
| `name` | Unique identifier for the skill (used for filtering and deduplication) |
| `description` | When and why to use this skill. This is shown in the catalog and guides the model's activation decision. |

### Body Content

The markdown body contains the actual instructions. It is lazy-loaded (read from disk only when the skill is activated via the `Read` tool), not stored in memory during discovery.

## Discovery

Skills are discovered from these locations, in precedence order:

1. **Project-level**: `{working_dir}/.agents/skills/`
2. **Configured paths**: directories listed in `skills.path`
3. **User-level**: `~/.agents/skills/`

### Discovery Rules

- Each subdirectory of `.agents/skills/` is checked for a `SKILL.md` file
- Excluded directories: `.git/`, `node_modules/`, `target/`
- Maximum scan depth: 4 levels
- Maximum directories scanned: 2000
- Name collisions within the same scope: first found wins
- Project skills override configured and user skills with the same name
- Configured skills override user skills with the same name
- Malformed skills produce diagnostics (logged as warnings/errors) but do not block other skills

### Example Discovery Output

```
Project: .agents/skills/debugging-cake/SKILL.md  ->  "debugging-cake"
Project: .agents/skills/evaluating-cake/SKILL.md  ->  "evaluating-cake"
User:    ~/.agents/skills/web-searching/SKILL.md   ->  "web-searching" (unless shadowed by project)
```

## Prompt Integration

Discovered skills appear in an initial developer context message as XML:

```xml
## Skills

<skill_instructions>
The following skills provide specialized instructions for specific tasks.
When a task matches a skill's description, use your file-read tool to load
the SKILL.md at the listed location before proceeding.
When a skill references relative paths, resolve them against the skill's
directory (the parent of SKILL.md) and use absolute paths in tool calls.
</skill_instructions>

<available_skills>
  <skill>
    <name>debugging-cake</name>
    <description>How to investigate and debug issues with the cake CLI tool...</description>
    <location>/Users/alice/Projects/cake/.agents/skills/debugging-cake/SKILL.md</location>
  </skill>
  <skill>
    <name>evaluating-cake</name>
    <description>Evaluate cake CLI session performance...</description>
    <location>/Users/alice/Projects/cake/.agents/skills/evaluating-cake/SKILL.md</location>
  </skill>
</available_skills>
```

The model sees this catalog and decides when to activate a skill based on the task at hand.

## Activation

When the model determines a skill is relevant, it calls the `Read` tool with the skill's `location` path. For example:

```json
{
  "path": "/Users/alice/Projects/cake/.agents/skills/debugging-cake/SKILL.md"
}
```

The Read tool returns the full file content (frontmatter + body), which the model then uses to guide its behavior.

### Deduplication

Once a skill is activated in a session, re-reading it returns a lightweight message instead of the full content:

```
Skill 'debugging-cake' is already active in this session.
Its instructions are already in the conversation context.
```

This prevents token waste and duplicate instructions when:
- The model reads the same skill multiple times in one session
- A session is resumed or continued (activated skills are reconstructed from conversation history)

Deduplication works by:
1. Tracking skill locations (path -> name mapping) in the `Agent`
2. Maintaining an `activated_skills` set shared across concurrent tool executions
3. Checking the set before executing a Read on a known skill path
4. Reconstructing the set from session history when resuming/continuing

## Configuration

### CLI Flags

| Flag | Description |
|------|-------------|
| `--no-skills` | Disable all skills for this session |
| `--skills name1,name2` | Only load specific skills (comma-separated) |

### Settings TOML

Add a `[skills]` section to `settings.toml`:

```toml
# Global: ~/.config/cake/settings.toml
# Project: .cake/settings.toml

[skills]
disabled = false
only = ["debugging-cake", "evaluating-cake"]
path = "~/my-skills:/shared/team-skills"
```

| Field | Description |
|-------|-------------|
| `disabled` | If `true`, disable all skills by default |
| `only` | List of skill names to load (empty = all) |
| `path` | Additional directories containing skills. Use colon-separated paths, semicolon on Windows, and `~` for the home directory |

### Precedence

Configuration is resolved with the following precedence (highest to lowest):

1. `--no-skills` CLI flag
2. `--skills name1,name2` CLI flag
3. `skills.only` in settings.toml
4. `skills.disabled = true` in settings.toml
5. Default: load all discovered skills

## Path Validation

Skill directories are automatically allowlisted for read access. The Read tool can access:

- The current working directory (read-write)
- Temp directories (read-write)
- Directories added via `--add-dir` (read-only)
- **Skill base directories** (read-only, automatically added)

This means the model can read skill files without needing `--add-dir` flags.

## Authoring Skills

To create a new skill:

1. Create a directory under `.agents/skills/` (project-level) or `~/.agents/skills/` (user-level)
2. Add a `SKILL.md` file with YAML frontmatter and markdown body
3. Ensure the `name` and `description` fields are present

### Best Practices

- **Description is critical**: Write it as instructions to the model about when to use the skill
- **Be specific**: The description should mention concrete trigger conditions
- **Keep it focused**: One skill per domain/task
- **Use absolute paths**: When a skill references files, tell the model to resolve relative paths against the skill directory

### Example Skill

```yaml
---
name: code-review
description: |
  Use this skill when the user asks for a code review, asks to check code quality,
  or mentions reviewing a pull request or diff.
---

# Code Review Guidelines

## Checklist

- [ ] Does the code follow the project's style guide?
- [ ] Are error cases handled appropriately?
- [ ] Is there adequate test coverage?
- [ ] Are there any security concerns?

## Output Format

Provide findings as:
1. **Critical** - Must fix before merging
2. **Suggestions** - Recommended improvements
3. **Nits** - Minor style issues
```

## Implementation

### Key Types

```rust
// src/config/skills.rs
pub struct Skill {
    pub name: String,
    pub description: String,
    pub location: PathBuf,
    pub base_directory: PathBuf,
    pub scope: SkillScope,
}

pub enum SkillScope {
    Project,
    User,
}

pub struct SkillCatalog {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

pub enum SkillConfig {
    All,
    Disabled,
    Only(Vec<String>),
}
```

### Key Functions

```rust
// Discover skills from filesystem
pub fn discover_skills(working_dir: &Path) -> SkillCatalog;

// Parse a SKILL.md file
impl Skill {
    pub fn parse(path: &Path, scope: SkillScope) -> Result<Self, SkillDiagnostic>;
}

// Generate XML catalog for prompt context
impl SkillCatalog {
    pub fn to_prompt_xml(&self) -> String;
    pub fn filter_to(&mut self, skill_names: &[String]);
}

// Resolve configuration from CLI and settings
impl SettingsLoader {
    pub fn resolve_skill_config(
        no_skills: bool,
        skills_flag: Option<&str>,
        settings: &SkillSettings,
    ) -> SkillConfig;
}
```

### Integration Flow

1. **`main.rs`**: Call `discover_skills()`, apply `SkillConfig`, pass catalog to `build_initial_prompt_messages()`
2. **`prompts/mod.rs`**: Emit `<available_skills>` XML in a developer context message if skills exist
3. **`tools/mod.rs`**: Register skill base directories for path validation via `set_skill_dirs()`
4. **`agent.rs`**: Check Read tool paths against `skill_locations`; deduplicate via `activated_skills`

## Testing

The skills system includes tests for:

- **Parsing**: Valid skills, malformed YAML, missing fields, multiline descriptions
- **Discovery**: Project skills, user skills, collision resolution, excluded directories, max depth
- **XML generation**: Format, escaping, empty catalog
- **Configuration precedence**: CLI flags override settings
- **Deduplication**: Same skill read twice returns lightweight message

## Related Documentation

- [prompts.md](./prompts.md): System prompt construction including skill catalog
- [settings.md](./settings.md): TOML configuration including `[skills]` section
- [tools.md](./tools.md): Read tool and path validation
- [session-management.md](./session-management.md): Session persistence and resume behavior
