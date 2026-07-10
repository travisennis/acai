# Codebase Research Report: install-skill Command (CLI Subcommand)

## Executive Summary

This report documents the current architecture of the acai codebase relevant to implementing a new `install-skill` CLI subcommand. Unlike REPL commands (like `/init-project`), this would be a standalone CLI command invoked as `acai install-skill`.

## Current CLI Architecture

**Location:** `source/index.ts`

The main entry point uses `parseArgs` from `node:util`:

```typescript
const parsed = syncTry(() =>
  parseArgs({
    options: {
      model: { type: "string", short: "m" },
      prompt: { type: "string", short: "p" },
      continue: { type: "boolean", default: false },
      resume: { type: "boolean", default: false },
      "add-dir": { type: "string", multiple: true },
      "no-skills": { type: "boolean", default: false },
      "no-session": { type: "boolean", default: false },
      help: { type: "boolean", short: "h" },
      version: { type: "boolean", short: "v" },
    },
    allowPositionals: true,
  }),
);

const flags = parsed.unwrap().values;
const input = parsed.unwrap().positionals;
```

### Current Execution Flow

1. **Early exits** (`handleEarlyExits`): `--version`, `--help`
2. **Resume/continue**: `--continue` or `--resume` flags
3. **CLI mode**: If `-p` flag or first positional exists → run single prompt and exit
4. **REPL mode**: Default - run interactive session

### Adding Subcommands

To add `acai install-skill`, we need to:

1. **Modify argument parsing** in `source/index.ts`:
   - Add new options for `install-skill` subcommand
   - Check if first positional is a known subcommand (e.g., `install-skill`)
   - If subcommand detected, handle it and exit early

2. **Create new command module**:
   - New file: `source/cli-commands/install-skill.ts`
   - Accept `-r/--repo` and `--skill-path` arguments
   - Perform the installation logic

3. **Update help text**:
   - Add subcommand to main help

## Skills Storage Locations

**Location:** `source/skills/index.ts`

Skills are loaded from:
- **Global user skills:** `~/.agents/skills/` (source: "user")
- **Project skills:** `./.agents/skills/` (source: "project")
- **Deprecated:** `~/.acai/skills/` and `./.acai/skills/` (shows warnings)

Each skill needs `SKILL.md` with frontmatter containing:
- `name`: skill name (lowercase, alphanumeric, hyphens)
- `description`: skill description
- Optional: `user-invocable`, `allowed-tools`, `arguments`, `examples`

## Implementation Strategy

### Option 1: Check first positional before main flow

In `source/index.ts`, after parsing args:

```typescript
// Check for subcommands
const subcommand = input.at(0);
if (subcommand === "install-skill") {
  // Run install-skill and exit
  await runInstallSkillCommand(input.slice(1), flags);
  return;
}
```

### Option 2: Add subcommand to parseArgs options

Add a subcommand option and check its value:

```typescript
const parsed = syncTry(() =>
  parseArgs({
    options: {
      // ... existing options
      "install-skill": { type: "boolean", default: false },
      // or use positional for subcommand
    },
    allowPositionals: true,
  }),
);
```

### Recommended Approach

**Option 1** is simpler and more extensible for future subcommands.

## Files to Create/Modify

### New Files:
- `source/cli-commands/install-skill/index.ts` - Main command implementation
- `source/cli-commands/install-skill/utils.ts` - Helper functions

### Modified Files:
- `source/index.ts` - Add subcommand detection and routing
- `ARCHITECTURE.md` - Document new subcommand architecture
- `README.md` - Document new command

## Dependencies Available

- `Executor` class for git clone operations (`source/execution/index.ts`)
- `select` for user prompts (`source/terminal/select-prompt.ts`)
- `loadSkills` function for checking existing skills (`source/skills/index.ts`)
- Skill validation functions (`source/skills/index.ts:38-88`)

## Edge Cases to Handle

1. Invalid GitHub repo URL or name
2. Network errors when cloning
3. Invalid skill name (must match directory, lowercase alphanumeric with hyphens)
4. Skill name conflicts with existing skills
5. Empty repos or repos without SKILL.md
6. Permission errors writing to global/project directories
7. Invalid `--skill-path` that doesn't exist in repo

## Comparison: REPL vs CLI Subcommand

| Aspect | REPL Command | CLI Subcommand |
|--------|--------------|----------------|
| Invocation | `/command` in REPL | `acai command` |
| Arguments | After command name | After subcommand |
| Exit | Returns to REPL | Exits immediately |
| User input | Via TUI | TTY + flags |
| Examples | `/init-project`, `/help` | `install-skill` (new) |

The key difference: CLI subcommands run immediately and exit, while REPL commands run within the interactive session.
