# AGENTS.md - Agentic Coding Guidelines for Acai

## Project Overview

**Acai** is an AI-powered coding assistant written in Rust that integrates with OpenRouter's Responses API for multi-provider AI access. Default model: MiniMax MiniMax-M2.5.

### Tech Stack
- **Language**: Rust (2021 edition)
- **Async Runtime**: Tokio with full features
- **CLI**: Clap with derive macros
- **HTTP**: Reqwest with JSON support
- **Serialization**: Serde + serde_json
- **Logging**: Log4rs
- **Error Handling**: Anyhow + Thiserror

## Build/Lint/Test Commands

```bash
# Build release binary
cargo build --release

# Run tests
cargo test

# Run tests for a specific module
cargo test <module_name>

# Lint with clippy
cargo clippy

# Strict linting (pedantic + unwrap/expect warnings)
just clippy-strict

# Update dependencies
just update-dependencies
```

## Running the App

```bash
# Set API key
export OPENROUTER_API_KEY=your_key_here

# Run binary directly
./target/release/acai instruct --prompt "Your prompt here"

# Or with cargo
cargo run --release -- instruct --prompt "Your prompt here"

# Available options:
#   --model <MODEL>      AI model to use (default: minimax/minimax-m2.5)
#   --temperature <0-1>  Creativity level
#   --max-tokens         Max tokens in response
#   -p, --prompt         Your instruction
```

## Custom Tools

No custom tools defined in `.acai/tools/`.

## Code Style Guidelines

- **Formatting**: Run `cargo fmt` before commits
- **Imports**: Use absolute paths within crate (`crate::module::Item`)
- **Types**: Use PascalCase for types, snake_case for functions/variables
- **Derives**: Always derive `Debug`, `Clone` for structs; use `Serialize`/`Deserialize` for data models
- **Error Handling**: Use `thiserror` for custom errors, `anyhow` for application errors
- **Async**: Prefer `async fn` with Tokio; use `?` for error propagation
- **Linting**: Must pass `cargo clippy -- -Dclippy::pedantic`
- **Avoid**: `unwrap()`, `expect()` - use proper error handling instead
- **Documentation**: Add doc comments (`///`) for public APIs

## Commit Format

**Yes - Conventional Commits required**

```bash
feat(<scope>): add new feature
fix(<scope>): resolve bug
docs(<scope>): update documentation
perf(<scope>): performance improvement
refactor(<scope>): code refactoring
style(<scope>): formatting changes
test(<scope>): add tests
chore: maintenance tasks
```

Use `git-cliff` for changelog generation. Breaking changes: `feat(<scope>)!: breaking change`

## Branch Strategy

- **Main branch**: `master`
- **Feature branches**: `feature/<description>`
- **Bug fixes**: `fix/<description>`
- **PRs**: Target `master`

## PR Requirements

1. Run tests: `cargo test`
2. Run linters: `cargo clippy` (strict preferred)
3. Format code: `cargo fmt`
4. Write tests for new functionality
5. Update README.md if CLI interface changes

## Additional Notes

- Config stored in `~/.cache/acai/` (see `src/config/data_dir.rs`)
- Logs at `~/.cache/acai/acai.log`
- API key required via `OPENROUTER_API_KEY` env var
