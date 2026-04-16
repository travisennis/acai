# Install required development tools
setup:
    @echo "Checking Rust installation..."
    @which rustc > /dev/null || { echo "ERROR: Rust not installed. Install from https://rustup.rs"; exit 1; }
    @echo "Installing required cargo tools..."
    cargo install cargo-edit --quiet 2>/dev/null || true
    cargo install cargo-deny --quiet 2>/dev/null || true
    cargo install cargo-llvm-cov --quiet 2>/dev/null || true
    cargo install prek --quiet 2>/dev/null || true
    cargo install --locked cocogitto --quiet 2>/dev/null || true
    @echo "Setup complete! Run 'just --list' to see available commands."

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
    cargo clippy --all-targets --all-features -- -D warnings

# Run tests
test:
    cargo test --quiet

# Lint for use of super::/self:: in production code (test modules use super::* is allowed)
lint-imports:
    @grep -rn 'use super::' src/ --include='*.rs' | grep -v 'use super::\*;' | { if grep -q .; then echo "ERROR: Use crate:: paths, not super:: in production code. Found:"; grep -rn 'use super::' src/ --include='*.rs' | grep -v 'use super::\*;'; exit 1; fi; }
    @! grep -rn 'use self::' src/ --include='*.rs' | grep -q . || true
    @echo "Import lint passed!"

# Run all checks (use in CI)
ci: fmt-check clippy-strict test lint-imports
    echo "All checks passed!"

# Recreate full CI pipeline locally (matches GitHub Actions)
ci-full: fmt-check clippy-strict test lint-imports deny doc build
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
