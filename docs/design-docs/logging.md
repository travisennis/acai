# Logging

This document describes how logging works in the acai project.

## Overview

The project uses the **`log`** crate combined with **`log4rs`** for logging configuration.

## Dependencies

- **`log`** (v0.4.29) - The standard logging facade in Rust
- **`log4rs`** (v1.4.0) - A logging framework that provides flexible configuration (similar to Log4j)

## Logging Architecture

The project uses a **dual-output logging** setup:

| Output | Log Levels | Destination |
|--------|------------|-------------|
| **stderr** | `error!`, `warn!`, `info!` | Console (via `ConsoleAppender`) |
| **file** | `error!`, `warn!`, `info!`, `debug!`, `trace!` | `acai.log` in cache directory |

### Log Format

```
{d(%Y-%m-%d %H:%M:%S)} | {l:5.5} | {f}:{L} — {m}{n}
```

Example output:
```
2026-02-27 10:30:45 | INFO | main:42 — data dir set: /Users/travis/.cache/acai
```

## Initialization

Logging is configured in `src/main.rs` at startup:

```rust
let _ = logger::configure(&data_dir.get_cache_dir());
```

The log file path is `<cache_dir>/acai.log`.

## Usage

Throughout the codebase, use the standard `log` crate macros:

```rust
use log::{info, error, debug, warn, trace};

info!("data dir set: {}", path);
error!("Failed to connect: {}", err);
debug!("Processing request: {:?}", request);
warn!("Deprecated feature used");
trace!("Detailed trace information");
```

## Important Notes

- The default **console (stderr) level is `Info`** — so you'll only see `info!`, `warn!`, and `error!` in the terminal
- **File logging captures everything down to `trace!`**, which is useful for debugging issues
- The implementation is located in `src/logger.rs`
