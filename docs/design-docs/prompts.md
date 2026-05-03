# Prompts Module

The `prompts` module builds the stable system prompt and the mutable context messages derived from `AGENTS.md` files, discovered skills, and environment context.

## Overview

The initial prompt is sent as multiple conversation messages:

1. **System**: Stable identity, "You are cake. You are running as a coding agent..."
2. **Developer context**: Project-specific instructions from `AGENTS.md` files
3. **Developer context**: Available skills catalog with activation instructions
4. **Developer context**: Environment context such as working directory and date
5. **Capabilities**: Implicitly defined by the available tools

For the Responses API, mutable context is sent as individual `developer` role messages in the input array. For Chat Completions, mutable context is folded into the first user message for compatibility with OpenAI-compatible providers that do not support developer role messages consistently.

Each invocation also appends `prompt_context` audit records to the session file
for the mutable context it used. Those records are not replayed on
continue/resume/fork; fresh context is rebuilt and appended for the new
invocation.

The module provides these public functions:

```rust
pub fn build_system_prompt() -> String

pub fn build_initial_prompt_messages(
    working_dir: &Path,
    agents_files: &[AgentsFile],
    skill_catalog: &SkillCatalog,
) -> Vec<(Role, String)>
```

## AGENTS.md Files

Cake reads instructions from three locations:

1. **User-level**: `~/.cake/AGENTS.md` — Personal preferences applicable to all projects
2. **XDG config**: `~/.config/AGENTS.md` — XDG-standard location for global instructions
3. **Project-level**: `./AGENTS.md` — Project-specific instructions

All files are optional. If present and non-empty, their contents are injected into a developer context message.

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

If any skills were discovered, a "Skills" section is emitted as a developer context message:

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

### Additional Context Section

If any `AGENTS.md` files have non-empty content, an additional context section is emitted as a developer context message:

```markdown
## Additional Context

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

With both files present, prompt construction returns separate messages:

```markdown
system:
You are cake. You are running as a coding agent in a CLI on the user's computer.

---

developer:
## Additional Context

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
system:
You are cake. You are running as a coding agent in a CLI on the user's computer.

---

developer:
Current working directory: /project
Today's date: 2026-05-03
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
- Avoid empty additional context sections
- Reduce token usage
- Keep the prompt clean

## Related Documentation

- [cli.md](./cli.md): CLI layer triggers prompt construction via `build_initial_prompt_messages()`
- [session-management.md](./session-management.md): AGENTS.md files are read during session initialization
- [tools.md](./tools.md): Tool definitions are included alongside prompts in API requests

## Integration

The prompt construction flow:

1. **`main.rs`** calls `data_dir.read_agents_files(&current_dir)`
2. **`config::DataDir`** reads and parses `~/.cake/AGENTS.md`, `~/.config/AGENTS.md`, and `./AGENTS.md`
3. **`main.rs`** calls `discover_skills(&current_dir)` to find available skills
4. **`main.rs`** passes `current_dir`, `agents_files`, and `skill_catalog` to `build_initial_prompt_messages()`
5. **`prompts`** constructs a stable system message plus separate mutable context messages
6. **`clients::responses`** sends mutable context as developer messages; **`clients::chat_completions`** folds mutable context into the first user message

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

- **Empty agents files**: No additional context section added
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
    let messages = build_initial_prompt_messages(Path::new("/tmp"), &files, &SkillCatalog::empty());
    let context = &messages[1].1;
    assert!(context.contains("## Additional Context"));
    assert!(context.contains("~/.cake/AGENTS.md"));
    assert!(context.contains("./AGENTS.md"));
    assert!(context.contains("User instructions"));
    assert!(context.contains("Project instructions"));
}

#[test]
fn empty_content_skipped() {
    let files = vec![
        AgentsFile { path: "~/.cake/AGENTS.md".to_string(), content: String::new() },
        AgentsFile { path: "./AGENTS.md".to_string(), content: "   ".to_string() },
    ];
    let messages = build_initial_prompt_messages(Path::new("/tmp"), &files, &SkillCatalog::empty());
    assert_eq!(messages.len(), 2); // system + environment context
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
