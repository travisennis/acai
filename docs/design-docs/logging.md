# Logging

This document describes how logging works in the cake project.

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
| **file** | `error!`, `warn!`, `info!`, `debug!`, `trace!` | `~/.cache/cake/cake.YYYY-MM-DD.log` |

### Log Rotation

Log files rotate **daily** with automatic cleanup:

- Files are named `cake.YYYY-MM-DD.log` (e.g., `cake.2024-01-15.log`)
- **Maximum 7 files retained** - oldest files are automatically deleted
- This prevents unbounded disk usage while preserving recent history

### Log Format

```
YYYY-MM-DD HH:MM:SS | LEVEL | file:line — message
```

Example output:
```
2024-01-15 10:30:45 | INFO | main:42 — data dir set: /Users/travis/.cache/cake
```

### Log Levels

| Level | Default | With `RUST_LOG=cake=trace` |
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

The log files are written to `<cache_dir>/cake.YYYY-MM-DD.log`.

## Usage

Throughout the codebase, use the `tracing` macros:

```rust
use tracing::{info, error, debug, warn, trace};

info!("data dir set: {}", path);
error!("Failed to connect: {}", err);
debug!("Processing request: {:?}", request);
warn!("Deprecated feature used");
trace!("Detailed trace information");  // Only with RUST_LOG=cake=trace
```

## Enabling Verbose Logging

To enable debug and trace logs, set the `RUST_LOG` environment variable:

```bash
# Enable trace logs for cake
RUST_LOG=cake=trace cake "your prompt"

# Enable debug logs
RUST_LOG=cake=debug cake "your prompt"

# Enable trace logs for all crates (very verbose)
RUST_LOG=trace cake "your prompt"
```

## Log File Location

Log files are stored in the cache directory:

- **macOS/Linux**: `~/.cache/cake/`
- Files follow the pattern: `cake.YYYY-MM-DD.log`

To view recent logs:

```bash
# View today's log
cat ~/.cache/cake/cake.$(date +%Y-%m-%d).log

# View all logs
ls -la ~/.cache/cake/cake.*.log
```

## Implementation Details

The logging implementation is in `src/logger.rs`:

```rust
pub fn configure(log_path: &Path) -> Result<(), Error> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("cake=info"));

    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("cake")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_path)?;

    // ... subscriber setup
}
```

Key features:

- **`EnvFilter`**: Respects `RUST_LOG` environment variable, defaults to `cake=info`
- **`RollingFileAppender`**: Daily rotation with 7-day retention
- **Non-blocking**: Async-safe writes that don't block the Tokio runtime