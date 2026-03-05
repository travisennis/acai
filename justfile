# Check code formatting (use in CI)
fmt-check:
    cargo fmt -- --check

# Auto-fix formatting
fmt:
    cargo fmt

# Run clippy with workspace lints (configured in Cargo.toml)
clippy:
    cargo clippy

# Ultra-strict clippy for CI (deny all warnings, lint all targets)
clippy-strict:
    cargo clippy --all-targets -- -D warnings

# Run tests
test:
    cargo test

# Run all checks (use in CI)
ci: fmt-check clippy-strict test
    echo "All checks passed!"

update-dependencies:
    cargo upgrade -i allow && cargo update    
