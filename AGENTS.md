# AGENTS.md

## Project Overview

acai is an AI coding assistant CLI that:
- Integrates with LLMs via an API compatible for with OpenAI Chat Completions or the Responses API.
- Executes tools (Bash, Read, Edit, Write) in a sandboxed environment
- Manages conversation sessions with continue/resume/fork capabilities
- Uses OS-level sandboxing (macOS Seatbelt, Linux Landlock)

**Core mechanism**: The agent loop lets the model execute tools, receive results, and continue until it returns a final response.

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

- Run the `Full CI check` command when you complete a task to make sure the code is correct.

---

## Debugging the App

When asked to debug the app, read the `debugging-acai` skill in `.agents/skills/debugging-acai/SKILL.md`

---

## Evaluating the App

When asked to evaluate the app, read the `evaluating-acai` skill in `.agents/skills/evaluating-acai/SKILL.md`

---

## Commit Conventions

This project uses [Conventional Commits](https://www.conventionalcommits.org/). Commit messages are validated by a `commit-msg` hook.

**Format:** `<type>[(scope)]: <description>`

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`

**Recommended Scopes** (aligned with architecture):

| Scope | Description |
|-------|-------------|
| `cli` | Command-line interface and argument parsing |
| `agent` | Agent orchestration, conversation loop, tool execution |
| `responses` | Responses API backend |
| `chat` | Chat Completions API backend |
| `tools` | Tool definitions (Bash, Read, Edit, Write, etc.) |
| `sandbox` | Sandbox implementations (Seatbelt, Landlock) |
| `config` | Configuration, sessions, data directory |
| `session` | Session persistence and management |
| `model` | Model configuration and API types |
| `prompts` | System prompt construction, AGENTS.md integration |
| `logger` | Logging configuration |
| `docs` | Documentation changes |
| `tests` | Test files and test infrastructure |

**Examples:**
```
feat(cli): add --verbose flag
fix(agent): handle timeout correctly
docs: update ARCHITECTURE.md with new module
refactor(tools): extract path validation into shared function
```

---

## Code Style Guidelines

- **Error Handling**: Use `thiserror` for custom errors, `anyhow` for application errors
- **Async**: Prefer `async fn` with Tokio; use `?` for error propagation

---

## Additional Notes

- Config stored in `~/.cache/acai/` and `.acai` (see `src/config/data_dir.rs`)
- Data directory can be overridden via `ACAI_DATA_DIR` environment variable
- Logs at `~/.cache/acai/acai.YYYY-MM-DD.log` (daily rotation, 7-day retention)
- API key required via environment variable (default: `OPENCODE_ZEN_API_TOKEN`)
