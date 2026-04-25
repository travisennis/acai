# ADR 002: Agent Skills System

**Status:** Accepted  
**Date:** 2025-04-25

## Context

As cake evolved, users needed a way to provide specialized instructions for specific tasks without bloating the system prompt. For example, debugging the cake CLI itself requires different procedures than evaluating a session or creating an EPUB. Pre-loading all possible instructions into the system prompt wastes tokens and overwhelms the model with irrelevant context.

We evaluated several approaches for making specialized knowledge available to the agent on demand.

## Decision

We implement a skills system with the following design:

1. **Discovery**: Skills are discovered from `.agents/skills/<skill-name>/SKILL.md` directories at both project (`./.agents/skills/`) and user (`~/.agents/skills/`) levels.
2. **Parsing**: Each `SKILL.md` file contains YAML frontmatter with `name` and `description` fields, followed by markdown body content.
3. **Catalog disclosure**: Discovered skills are listed in the system prompt as an XML `<available_skills>` catalog, telling the model which skills exist and where to find them.
4. **Lazy activation**: The model uses the existing `Read` tool to load a `SKILL.md` file when its description matches the current task. The skill content is then in the conversation context.
5. **Deduplication**: Once a skill is read in a session, subsequent reads return a lightweight "already active" message instead of re-reading the file, saving tokens and preventing duplicate instructions.
6. **Configuration**: Users can disable skills (`--no-skills`), filter to specific skills (`--skills name1,name2`), or configure defaults in `settings.toml`.

## Rationale

- **On-demand loading**: Skills are loaded only when needed, keeping the system prompt minimal and focused.
- **Existing tool reuse**: The model already has a `Read` tool. Skill activation uses it without adding new mechanics.
- **Familiar format**: YAML frontmatter is widely understood (Jekyll, Hugo, etc.), making skill authoring accessible.
- **Token efficiency**: Deduplication prevents the same skill instructions from being repeated in conversation history.
- **Session persistence**: Activated skills are tracked across session resume/continue via conversation history scanning.

## Consequences

- **Positive**: Reduced system prompt size, better model focus, easy to add new specialized knowledge
- **Positive**: Skills are plain markdown files with YAML frontmatter, no special tooling needed to author them
- **Negative**: Requires the model to make an extra tool call to activate a skill before using it
- **Negative**: Skill discovery has a small filesystem scan cost at startup (mitigated by depth/directory limits)

## Alternatives Considered

- **Inline all skills in system prompt**: Rejected because it bloats the prompt with potentially irrelevant instructions and increases token costs.
- **Dedicated skill activation tool**: Rejected because it duplicates the existing `Read` tool. Using `Read` keeps the tool surface minimal.
- **Auto-activate based on keyword matching**: Rejected because it is brittle and could activate skills incorrectly. The model makes the activation decision based on its understanding of the task.
- **Store skill content in settings.toml**: Rejected because markdown files are easier to author and version control than embedded TOML strings.

## References

- `docs/design-docs/skills.md` - Full feature documentation
- `src/config/skills.rs` - Skill discovery and parsing implementation
- `src/prompts/mod.rs` - System prompt integration
- `src/clients/agent.rs` - Activation deduplication logic
