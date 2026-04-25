# Prompts Module

The `prompts` module handles system prompt construction, integrating instructions from `AGENTS.md` files at user, XDG config, and project levels.

## Overview

The system prompt is the first message sent to the AI model, establishing:

1. **Identity**: "You are cake. You are running as a coding agent..."
2. **Context**: Project-specific instructions from `AGENTS.md` files
3. **Skills**: Available skills catalog with activation instructions
4. **Capabilities**: Implicitly defined by the available tools

The module provides a single public function:

```rust
pub fn build_system_prompt(
    working_dir: &Path,
    agents_files: &[AgentsFile],
    skill_catalog: &SkillCatalog,
) -> String
```

## AGENTS.md Files

Cake reads instructions from three locations:

1. **User-level**: `~/.cake/AGENTS.md` — Personal preferences applicable to all projects
2. **XDG config**: `~/.config/AGENTS.md` — XDG-standard location for global instructions
3. **Project-level**: `./AGENTS.md` — Project-specific instructions

All files are optional. If present and non-empty, their contents are injected into the system prompt.

### AgentsFile Struct

```rust
pub struct AgentsFile {
    pub path: String,    // Display path (e.g., "~/.cake/AGENTS.md")
    pub content: String, // File contents
}
```

This struct is defined in the `config` module and populated by `DataDir::read_agents_files()`.

## Prompt Construction

### Base Prompt

The base system prompt establishes the AI's identity:

```rust
"You are cake. You are running as a coding agent in a CLI on the user's computer."
```

### Skills Section

If any skills were discovered, a "Skills" section is appended after the AGENTS.md context:

```markdown
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
    <location>/path/to/SKILL.md</location>
  </skill>
</available_skills>
```

Skills are lazy-loaded: the model reads the `SKILL.md` file via the Read tool when it determines the skill is relevant. Once activated, the skill is deduplicated (subsequent reads return a lightweight "already active" message).

For full details on the skills system, see [skills.md](./skills.md).

### Project Context Section

If any `AGENTS.md` files have non-empty content, a "Project Context" section is appended:

```markdown
## Project Context:

### ~/.cake/AGENTS.md

<instructions>
User-level instructions here...
</instructions>

### ~/.config/AGENTS.md

<instructions>
XDG config instructions here...
</instructions>

### ./AGENTS.md

<instructions>
Project-level instructions here...
</instructions>
```

Empty or whitespace-only files are skipped.

### Example Output

With both files present:

```markdown
You are cake. You are running as a coding agent in a CLI on the user's computer.

## Project Context:

### ~/.cake/AGENTS.md

<instructions>
Always format code with rustfmt before returning it.
Prefer anyhow for error handling.
</instructions>

### ./AGENTS.md

<instructions>
This project uses snake_case for all identifiers.
Run `cargo test` after making changes.
</instructions>
```

Without AGENTS.md files:

```markdown
You are cake. You are running as a coding agent in a CLI on the user's computer.
```

## Design Decisions

### XML-style Tags

Instructions are wrapped in `<instructions>` tags to:
- Clearly delimit user instructions from system text
- Help the model distinguish context from conversation
- Allow for future nested structure if needed

### File Path Display

The `path` field uses display paths like `~/.cake/AGENTS.md` rather than absolute paths:
- More readable for users
- Consistent across different machines
- Indicates the source (user vs. project level)

### Empty File Filtering

Files with only whitespace are filtered out to:
- Avoid empty "Project Context" sections
- Reduce token usage
- Keep the prompt clean

### No Working Directory Usage

The `_working_dir` parameter is currently unused but kept for:
- Future extensibility (project-specific logic)
- API stability

## Related Documentation

- [cli.md](./cli.md): CLI layer triggers prompt construction via `build_system_prompt()`
- [session-management.md](./session-management.md): AGENTS.md files are read during session initialization
- [tools.md](./tools.md): Tool definitions are included alongside prompts in API requests

## Integration

The prompt construction flow:

1. **`main.rs`** calls `data_dir.read_agents_files(&current_dir)`
2. **`config::DataDir`** reads and parses `~/.cake/AGENTS.md`, `~/.config/AGENTS.md`, and `./AGENTS.md`
3. **`main.rs`** calls `discover_skills(&current_dir)` to find available skills
4. **`main.rs`** passes `agents_files` and `skill_catalog` to `build_system_prompt()`
5. **`prompts`** constructs the final string with AGENTS.md context and skills catalog
6. **`clients::responses`** includes it as the first message in API requests

## Use Cases

### User-Level Instructions

Common patterns for `~/.cake/AGENTS.md`:

- **Code style preferences**: "Prefer functional programming style"
- **Default tools**: "Always run tests after editing code"
- **Error handling**: "Use anyhow for errors, thiserror for libraries"
- **Documentation**: "Add doc comments to all public items"

### XDG Config Instructions

Common patterns for `~/.config/AGENTS.md`:

- **Cross-tool preferences**: Instructions shared with other tools that read `~/.config/AGENTS.md`
- **Global defaults**: Same purpose as `~/.cake/AGENTS.md` but following the XDG Base Directory convention

### Project-Level Instructions

Common patterns for `./AGENTS.md`:

- **Architecture rules**: "Follow the layered architecture in ARCHITECTURE.md"
- **Testing requirements**: "All changes must include tests"
- **Build commands**: "Use `just build` instead of `cargo build`"
- **Project conventions**: "Use `crate::` for imports, never relative paths"

### Combined Context

Both files work together:

- User preferences apply everywhere
- Project rules override or extend for specific projects
- The AI sees both and applies them appropriately

## Testing

The module includes tests for:

- **Empty agents files**: No Project Context section added
- **With agents files**: Correct formatting and inclusion
- **Only user file**: Single file in context section
- **Empty content skipped**: Whitespace-only files ignored

Example tests:

```rust
#[test]
fn with_agents_files() {
    let files = vec![
        AgentsFile { path: "~/.cake/AGENTS.md".to_string(), content: "User instructions".to_string() },
        AgentsFile { path: "./AGENTS.md".to_string(), content: "Project instructions".to_string() },
    ];
    let prompt = build_system_prompt(Path::new("/tmp"), &files);
    assert!(prompt.contains("## Project Context:"));
    assert!(prompt.contains("~/.cake/AGENTS.md"));
    assert!(prompt.contains("./AGENTS.md"));
    assert!(prompt.contains("User instructions"));
    assert!(prompt.contains("Project instructions"));
}

#[test]
fn empty_content_skipped() {
    let files = vec![
        AgentsFile { path: "~/.cake/AGENTS.md".to_string(), content: String::new() },
        AgentsFile { path: "./AGENTS.md".to_string(), content: "   ".to_string() },
    ];
    let prompt = build_system_prompt(Path::new("/tmp"), &files);
    assert!(!prompt.contains("## Project Context:"));
}
```

## Future Enhancements

Potential improvements:

- **Dynamic prompts**: Include current git status, recent files
- **Template system**: Allow variable substitution in AGENTS.md
- **Conditional rules**: Different instructions based on file type
- **Validation**: Lint AGENTS.md and SKILL.md for common issues
- **Skill dependencies**: Allow skills to declare dependencies on other skills

These would be additions to the current simple, reliable approach rather than replacements.
