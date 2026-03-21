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

# Recreate full CI pipeline locally (matches GitHub Actions)
ci-full: fmt-check clippy-strict test deny doc build
    echo "Full CI pipeline passed!"

# Check for denied/advisory dependencies (requires cargo-deny)
deny:
    cargo deny check advisories

# Build documentation with warnings denied
doc:
    RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items

# Run tests with coverage (requires cargo-llvm-cov)
coverage:
    cargo llvm-cov --html

# Run coverage and open report
coverage-open:
    cargo llvm-cov --html --open
# Generate coverage in lcov format for CI
coverage-lcov:
    cargo llvm-cov --lcov --output-path lcov.info

update-dependencies:
    cargo upgrade -i allow && cargo update    

build:
    cargo build --release
