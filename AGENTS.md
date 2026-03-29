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
2. Read [CONTRIBUTING.md](CONTRIBUTING.md) to understand the rules of working in this code base
3. Read [ARCHITECTURE.md](ARCHITECTURE.md) to understand affected modules

### During Development
- Follow architectural invariants in [ARCHITECTURE.md](ARCHITECTURE.md)
- All code style enforced by clippy — if it compiles, it's compliant
- See Code Style Guidelines below for conventions not enforced by tooling

### After Completing a Task

A development task is considered "complete" only when ALL of the following are met:

1. Run `just ci` to verify all checks pass
2. Update affected documentation:
   - [ARCHITECTURE.md](ARCHITECTURE.md) if architecture changed
   - [README.md](README.md) if user-facing features changed
   - [docs/design-docs/index.md](docs/design-docs/index.md) if new design docs added
3. Provide summary of changes to user

---

## Build/Test/Run

```bash
# Build release binary
cargo build --release

# Run tests
cargo test

# Run tests for a specific module
cargo test <module_name>

# Run tests with coverage
just coverage

# Run coverage and open HTML report
just coverage-open

# Formatting
cargo fmt

# Linting
just clippy-strict

# Update dependencies
just update-dependencies

# Full CI check
just ci
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for:
- Running the application
- Development setup

---

## Debugging the App

When asked to debug the app, read the `debugging-acai` skill in `.agents/skills/debugging-acai/SKILL.md`

---

## Git Workflow

- **Never commit directly to the master branch** — verify current branch with `git branch` before committing
- Merge via feature branch + PR. Naming: `feat/xxx`, `fix/xxx`, `refactor/xxx`, `test/xxx`

When committing code first read [CONTRIBUTING.md](CONTRIBUTING.md)

---

## Code Style Guidelines

- **Error Handling**: Use `thiserror` for custom errors, `anyhow` for application errors
- **Async**: Prefer `async fn` with Tokio; use `?` for error propagation

---

## Additional Notes

- Config stored in `~/.cache/acai/` and `.acai` (see `src/config/data_dir.rs`)
- Logs at `~/.cache/acai/acai.log`
- API key required via environment variable (default: `OPENCODE_ZEN_API_TOKEN`)
