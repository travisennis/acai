---
status: accepted
date: 2026-07-02
decision-makers: Travis Ennis
---
# Skills System Architecture

## Context

The acai-ts CLI tool needs a skill system that allows extending the agent's capabilities with specialized knowledge and workflows. Skills should be discovered from filesystem locations, validated, and made available to the model for invocation.

## Decision

### Skill Directory Structure

Skills are stored in directories containing a `SKILL.md` file with YAML frontmatter and markdown body.

```
.skells/
  skill-name/
    SKILL.md        # Frontmatter + instructions
    supporting-files/
```

### SKILL.md Format

```markdown
---
name: skill-name
description: What this skill does
allowed-tools: ToolName1, ToolName2
user-invocable: true
disable-model-invocation: false
arguments: --flag <arg1> <arg2>
examples:
  - "skill-name --flag value"
---

Skill instructions in markdown...
```

### Discovery Locations

Skills are loaded from multiple sources in priority order:

1. **User global**: `~/.agents/skills/` (recursive, colon-path names)
2. **Project**: `.agents/skills/` (recursive, colon-path names)
3. **Claude compat**: `~/.claude/skills/` (single-level, dir-name)
4. **Codex compat**: `~/.codex/skills/` (recursive, dir-name)

Colon-path names allow namespacing: `db:migrate` resolves to `db/migrate/`.

### Skill Loading Process

```typescript
export class Skills {
  private skills: Skill[];

  getAll(): Skill[];
  getUserInvocable(): Skill[];  // For /skill command
  getModelInvocable(): Skill[]; // For tool invocation
}
```

### Model Invocation Flow

1. Model sees skills listed in system prompt
2. Model requests `Skill` tool with `skill` name and optional `args`
3. Tool reads SKILL.md, parses frontmatter, loads body
4. Lists supporting files in `<skill_resources>` tags
5. Replaces `{0}`, `{1}`, etc. placeholders with provided args
6. Returns skill content as text for model consumption
7. Skill marked as "activated" to prevent duplicate loading

### Deduplication

The `ActivatedSkillsTracker` prevents the same skill from being loaded multiple times:

```typescript
if (activatedSkillsTracker.has(skillName)) {
  return `Skill "${skillName}" is already loaded...`;
}
activatedSkillsTracker.add(skillName);
```

### Validation

Skills are validated on load:
- Name must match directory name, lowercase, hyphenated only
- Description required, 1-1024 characters
- Hidden files and symlink cycles are skipped

### Tool Definition

```typescript
const inputSchema = z.object({
  skill: z.string().describe('The skill name. E.g., "commit", "review-pr"'),
  args: z.string().optional().describe("Optional arguments for the skill"),
});
```

## Consequences

### Positive
- Compatible with Claude Code and Codex skill formats
- Colon-path names enable namespacing without file nesting
- File-based storage is simple and version-controllable
- Supports rich content with markdown formatting
- Deduplication prevents context pollution

### Negative
- Filesystem-based discovery has latency on startup
- No built-in skill versioning or dependencies
- Skill conflicts resolved by last-write-wins (project > user)

### Alternatives Considered

**Database Storage:** SQLite or similar for skill storage would add complexity without benefit. Skills are text files that benefit from version control. Rejected.

**Registry Service:** Central registry would require network access and introduce a dependency. Filesystem discovery is offline-first. Rejected.

**Plugin System:** A formal plugin system with manifest files and lifecycle hooks is overkill for the current use case. May be revisited if needs grow.

