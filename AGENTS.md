# AGENTS.md

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

## Running the App

```bash
# Set API key
export OPENROUTER_API_KEY=your_key_here

# Run binary directly
./target/release/acai instruct --prompt "Your prompt here"

# Or with cargo
cargo run --release -- instruct --prompt "Your prompt here"

# To get help
./target/release/acai instruct --help
```

## Code Style Guidelines

- **Imports**: Use absolute paths within crate (`crate::module::Item`)
- **Types**: Use PascalCase for types, snake_case for functions/variables
- **Derives**: Always derive `Debug`, `Clone` for structs; use `Serialize`/`Deserialize` for data models
- **Error Handling**: Use `thiserror` for custom errors, `anyhow` for application errors
- **Async**: Prefer `async fn` with Tokio; use `?` for error propagation

## Additional Notes

- Config stored in `~/.cache/acai/` (see `src/config/data_dir.rs`)
- Logs at `~/.cache/acai/acai.log`
- API key required via `OPENROUTER_API_KEY` env var
