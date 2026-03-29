# Contributing to Acai

Thank you for your interest in contributing to Acai! This document provides all the information you need to get started.

## Development Setup

### Prerequisites

- Rust and Cargo installed on your system
- Git

### Install Development Tools

```bash
# Install prek for git hooks
cargo install prek

# Install cocogitto for conventional commit validation
cargo install --locked cocogitto

# Install git hooks
prek install --hook-type pre-commit --hook-type commit-msg
```

Git hooks will automatically run:
- **pre-commit**: `cargo fmt -- --check` (formatting verification)
- **pre-commit**: `cargo clippy --all-targets -- -D warnings` (linting)
- **commit-msg**: `cog verify --file` (conventional commit validation)

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

# Full CI check
just ci
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

## Git Workflow

- **Never commit directly to the master branch** — verify current branch with `git branch` before committing
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

## Pull Request Process

1. Fork the repository
2. Create a new branch for your feature or bug fix (see Git Workflow naming conventions)
3. Make your changes and commit them following the commit conventions above
4. Write tests for new functionality
5. Ensure all CI checks pass (build, formatting, linting, tests)
6. Update affected documentation if needed
7. Submit a pull request
