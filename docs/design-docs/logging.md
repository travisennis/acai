# Logging

This document describes how logging works in the acai project.

## Overview

The project uses the **`tracing`** crate combined with **`tracing-subscriber`** and **`tracing-appender`** for structured logging with file rotation.

## Dependencies

- **`tracing`** (v0.1) - Modern instrumentation framework for Rust
- **`tracing-subscriber`** (v0.3) - Subscriber implementation with `env-filter` feature
- **`tracing-appender`** (v0.2) - File appender with rolling log support

## Logging Architecture

The project uses a **file-only logging** setup. All log levels are written to a rotating log file, keeping console output clean for user-facing messages.

| Output | Log Levels | Destination |
|--------|------------|-------------|
| **file** | `error!`, `warn!`, `info!`, `debug!`, `trace!` | `~/.cache/acai/acai.YYYY-MM-DD.log` |

### Log Rotation

Log files rotate **daily** with automatic cleanup:

- Files are named `acai.YYYY-MM-DD.log` (e.g., `acai.2024-01-15.log`)
- **Maximum 7 files retained** - oldest files are automatically deleted
- This prevents unbounded disk usage while preserving recent history

### Log Format

```
YYYY-MM-DD HH:MM:SS | LEVEL | file:line — message
```

Example output:
```
2024-01-15 10:30:45 | INFO | main:42 — data dir set: /Users/travis/.cache/acai
```

### Log Levels

| Level | Default | With `RUST_LOG=acai=trace` |
|-------|---------|----------------------------|
| `error!` | ✓ | ✓ |
| `warn!` | ✓ | ✓ |
| `info!` | ✓ | ✓ |
| `debug!` | ✗ | ✓ |
| `trace!` | ✗ | ✓ |

**Default level is INFO** - debug and trace logs are only emitted when explicitly enabled via environment variable.

## Initialization

Logging is configured in `src/main.rs` at startup:

```rust
let _ = logger::configure(&data_dir.get_cache_dir());
```

The log files are written to `<cache_dir>/acai.YYYY-MM-DD.log`.

## Usage

Throughout the codebase, use the `tracing` macros:

```rust
use tracing::{info, error, debug, warn, trace};

info!("data dir set: {}", path);
error!("Failed to connect: {}", err);
debug!("Processing request: {:?}", request);
warn!("Deprecated feature used");
trace!("Detailed trace information");  // Only with RUST_LOG=acai=trace
```

## Enabling Verbose Logging

To enable debug and trace logs, set the `RUST_LOG` environment variable:

```bash
# Enable trace logs for acai
RUST_LOG=acai=trace acai "your prompt"

# Enable debug logs
RUST_LOG=acai=debug acai "your prompt"

# Enable trace logs for all crates (very verbose)
RUST_LOG=trace acai "your prompt"
```

## Log File Location

Log files are stored in the cache directory:

- **macOS/Linux**: `~/.cache/acai/`
- Files follow the pattern: `acai.YYYY-MM-DD.log`

To view recent logs:

```bash
# View today's log
cat ~/.cache/acai/acai.$(date +%Y-%m-%d).log

# View all logs
ls -la ~/.cache/acai/acai.*.log
```

## Implementation Details

The logging implementation is in `src/logger.rs`:

```rust
pub fn configure(log_path: &Path) -> Result<(), Error> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("acai=info"));

    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("acai")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_path)?;

    // ... subscriber setup
}
```

Key features:

- **`EnvFilter`**: Respects `RUST_LOG` environment variable, defaults to `acai=info`
- **`RollingFileAppender`**: Daily rotation with 7-day retention
- **Non-blocking**: Async-safe writes that don't block the Tokio runtime