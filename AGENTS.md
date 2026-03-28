# AGENTS.md

## Project Overview

acai is an AI coding assistant CLI that:
- Integrates with LLMs via an API compatible for with OpenAI Chat Completions or the Responses API.
- Executes tools (Bash, Read, Edit, Write) in a sandboxed environment
- Manages conversation sessions with continue/resume/fork capabilities
- Uses OS-level sandboxing (macOS Seatbelt, Linux Landlock)

**Core mechanism**: The agent loop lets the model execute tools, receive results, and continue until it returns a final response.

## Repository Knowledge Map

### Architecture
- **[ARCHITECTURE.md](ARCHITECTURE.md)** — Architecture map: module structure, layering rules, dependency directions, cross-cutting concerns

### Design Documents
- **[docs/design-docs/index.md](docs/design-docs/index.md)** — Design document index (technical proposals & architecture decisions)

### References
- **[docs/references/](docs/references/)** — External guides, configuration docs, reference articles

---

## Development Workflow

### Before Starting a Task
1. Check the Repository Knowledge Map above for relevant documentation
2. Read [ARCHITECTURE.md](ARCHITECTURE.md) to understand affected modules
3. Review [current-issues.md](current-issues.md) for known patterns and anti-patterns

### During Development
- Follow architectural invariants in [ARCHITECTURE.md](ARCHITECTURE.md)
- All code style enforced by clippy — if it compiles, it's compliant
- See Code Style Guidelines below for conventions not enforced by tooling

### After Completing a Task
1. Run `just ci` to verify all checks pass
2. Update affected documentation:
   - [ARCHITECTURE.md](ARCHITECTURE.md) if architecture changed
   - [README.md](README.md) if user-facing features changed
   - [docs/design-docs/index.md](docs/design-docs/index.md) if new design docs added
3. Add entry to [CHANGELOG.md](CHANGELOG.md) if user-facing
4. Provide summary of changes to user

### Task Completion Criteria

A development task is considered "complete" only when ALL of the following are met:

1. ✅ CI checks all pass (`just ci`)
2. ✅ Affected docs updated
3. ✅ Results summary shown to the user

---

## Build/Test/Run

See [CONTRIBUTING.md](CONTRIBUTING.md) for:
- Build commands
- Test commands
- Running the application
- Development setup

---

## Code Style Guidelines

Most style is enforced by `cargo clippy`, `cargo fmt`, and `just lint-imports`. If it compiles and passes CI, it's compliant.

The following conventions are NOT enforced by tooling and must be followed manually:

- **Error Handling**: Use `thiserror` for custom errors, `anyhow` for application errors
- **Async**: Prefer `async fn` with Tokio; use `?` for error propagation

---

## Learning from Past Work

- **[current-issues.md](current-issues.md)** — Patterns and anti-patterns from past sessions
- **[MISTAKES.md](MISTAKES.md)** — Common mistakes to avoid (if exists)
- **[DESIRES.md](DESIRES.md)** — Missing context/tools that would help (if exists)
- **[LEARNINGS.md](LEARNINGS.md)** — Environment-specific learnings (if exists)

---

## Additional Notes

- Config stored in `~/.cache/acai/` (see `src/config/data_dir.rs`)
- Logs at `~/.cache/acai/acai.log`
- API key required via environment variable (default: `OPENCODE_ZEN_API_TOKEN`)
- For commit conventions, git workflow, and PR process, see [CONTRIBUTING.md](CONTRIBUTING.md)
- For debugging help, see the `debugging-acai` skill in `.agents/skills/debugging-acai/SKILL.md`
