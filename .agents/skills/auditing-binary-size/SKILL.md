---
name: auditing-binary-size
description: "Analyze and audit the release binary size to identify what's contributing to bloat. Use when asked to check binary size, audit binary bloat, investigate why the binary is large, or monitor binary size over time."
---

# Auditing Binary Size

Analyze the cake release binary to understand size contributors and identify optimization opportunities.

## Prerequisites

Ensure `cargo-bloat` is installed:

```bash
cargo install cargo-bloat
```

## Workflow

### 1. Build the release binary and check current size

```bash
cargo build --release
ls -lh target/release/cake
```

### 2. Analyze size by crate

Shows which dependencies contribute the most to the `.text` section:

```bash
cargo bloat --release --crates
```

### 3. Analyze size by function

Shows the largest individual functions:

```bash
cargo bloat --release -n 30
```

### 4. Review findings

The typical size breakdown for this project:

| Category | Typical Contributors |
|---|---|
| **TLS/Networking** | `aws_lc_sys`, `rustls`, `h2`, `hyper`, `reqwest` — usually the largest chunk |
| **Standard library** | `std` — unavoidable baseline |
| **Async runtime** | `tokio` — moderate |
| **CLI framework** | `clap_builder` — moderate |
| **Application code** | `cake` — relatively small |
| **Other deps** | `similar`, `regex_*`, `serde_json`, `tracing_subscriber`, etc. |

### 5. Check for easy wins

- **Symbols stripped?** The release profile should have `strip = true`. If not, that's ~1.3 MB of free savings with no performance cost (only downside: raw addresses in panic backtraces instead of function names).
- **Tokio features minimal?** Avoid `features = ["full"]`. Only enable what's used: `rt`, `rt-multi-thread`, `process`, `time`, `io-util`, `macros`.
- **Unused dependencies?** Run `cargo machete` to detect unused deps (install with `cargo install cargo-machete`).

### 6. Compare over time

To track whether binary size is growing, compare before and after changes:

```bash
ls -lh target/release/cake
cargo bloat --release --crates | head -20
```

## Current Release Profile

The release profile in `Cargo.toml` should include these optimizations:

```toml
[profile.release]
lto = true          # Link-time optimization (slower build, smaller/faster binary)
codegen-units = 1   # Single codegen unit (slower build, better optimization)
strip = true        # Strip symbols (free size reduction, ~1.3 MB savings)
```

## Key Tradeoffs to Be Aware Of

- **`rustls` vs `native-tls`**: `rustls` bundles ~1 MB of crypto (`aws_lc_sys` + `rustls`). Switching to `native-tls` uses the OS TLS stack (smaller binary) but introduces platform-dependent build behavior, especially on Linux with OpenSSL.
- **`opt-level = "z"`**: Optimizes for size over speed. Not recommended unless binary size is critical.
- **`strip = true`**: Only downside is panic backtraces show raw addresses instead of function names. Logs via `tracing` are unaffected.
