# AGENTS.md

## Project Overview

cake is an AI coding assistant CLI that:
- Written with Rust 2024 edition with Tokio async runtime
- Uses clap for CLI parsing, anyhow/thiserror for error, tracing for logging
- Use reqwest + serde/serde_json for HTTP/JSON
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

# Build and install to ~/bin
just install

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

# Verify Rust toolchain pins are synchronized
just rust-version-check
```
---

## Agent Instructions

- Run the `Full CI check` command when you complete a task to make sure the code is correct.
- Do not commit or push code unless explicitly asked to.

## Updating Rust Version

The project Rust toolchain is pinned in `rust-toolchain.toml`. When changing it:
- Update `rust-toolchain.toml`.
- Update matching project-toolchain pins in `.github/workflows/ci.yml`, `.github/workflows/release.yml`, and non-MSRV Rust jobs in `.github/workflows/scheduled.yml`.
- Leave the scheduled `MSRV Compatibility` job pinned to the supported minimum Rust version unless intentionally changing MSRV.
- Run `just rust-version-check` to verify pins are synchronized.
- Run `just ci` before finishing the change.

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

## ExecPlans

When writing complex features or significant refactors, use an ExecPlan (as described in .agents/PLANS.md) from design to implementation.

---

## Additional Notes

- Cache directory: `~/.cache/cake/` (logs, ephemeral data)
- Session directory: `~/.local/share/cake/sessions/` (conversation history)
- Both can be overridden via `CAKE_DATA_DIR` environment variable
- Project-level settings in `.cake/settings.toml`
- Logs at `~/.cache/cake/cake.YYYY-MM-DD.log` (daily rotation, 7-day retention)
