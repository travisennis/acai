# AGENTS.md

## Repository Knowledge Map

### Architecture

- **[ARCHITECTURE.md](ARCHITECTURE.md)** — Architecture map: module structure, layering rules, dependency directions, cross-cutting concerns

### Design Documents

- **[docs/design-docs/index.md](docs/design-docs/index.md)** — Design document index (technical proposals & architecture decisions)

### Spec

- **[docs/spec/index.md](docs/specs/index.md)** — Production specfiction index

### References

- **[docs/references/](docs/references/)** — External guides, configuration docs, reference articles

---

## Build/Lint/Test Commands

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
```

## Development Setup

Required tools for contributing:

```bash
# Install prek for git hooks
cargo install prek

# Install cocogitto for conventional commit validation
cargo install --locked cocogitto

# Install git hooks
prek install --hook-type pre-commit --hook-type commit-msg
```

## Running the App

```bash
# Set API key
export OPENROUTER_API_KEY=your_key_here

# Run binary directly
./target/release/acai "Your prompt here"

# Or with cargo
cargo run --release -- "Your prompt here"

# To get help
./target/release/acai --help
```

## Development Workflow

### Document-Driven Principle (Mandatory)

> ⛔ This is not a suggestion — it is a hard requirement. Skipping documentation steps = task failure.

**Before starting a task:** Check the "Repository Knowledge Map" above, find and read relevant docs before starting work.

**After completing a task:** Run the `managing-docs` skill to validate and update all project documentation (ARCHITECTURE.md, README, design-docs index, tech-debt, specs).

### Task Completion Criteria

A development task is considered "complete" only when ALL of the following are met:

1. ✅ CI checks all pass
2. ✅ Affected docs updated
3. ✅ New/modified code is traceable to a spec
4. ✅ Self-review results summary shown to the user

## Rules

- Run all checks (build, formatting, linting, and tests) at the completion of coding tasks. Verify changes compile and pass tests before finishing.
- If compilation fails, analyze the error output and fix syntax issues

## Code Style Guidelines

- **Imports**: Use absolute paths within crate (`crate::module::Item`)
- **Types**: Use PascalCase for types, snake_case for functions/variables
- **Derives**: Always derive `Debug`, `Clone` for structs; use `Serialize`/`Deserialize` for data models
- **Error Handling**: Use `thiserror` for custom errors, `anyhow` for application errors
- **Async**: Prefer `async fn` with Tokio; use `?` for error propagation

## Git Workflow

- **Never commit directly to the main branch** — verify current branch with `git branch` before committing
- Merge via feature branch + PR. Naming: `feat/xxx`, `fix/xxx`, `refactor/xxx`, `test/xxx`

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

## Additional Notes

- Config stored in `~/.cache/acai/` (see `src/config/data_dir.rs`)
- Logs at `~/.cache/acai/acai.log`
- API key required via `OPENROUTER_API_KEY` env var
