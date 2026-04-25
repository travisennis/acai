# cake

## Stack
- Rust 2024 edition with Tokio async runtime
- clap for CLI parsing, anyhow/thiserror for errors, tracing for logging
- reqwest + serde/serde_json for HTTP/JSON
- Optional Linux Landlock sandbox feature via landlock crate

## Architecture
The CLI entrypoint in src/main.rs parses args, resolves model config (src/config/model.rs), builds a system prompt (src/prompts/mod.rs), loads AGENTS.md via DataDir (src/config/data_dir.rs), and orchestrates an Agent from src/clients/mod.rs. It manages sessions (persisted JSONL) and worktrees (src/config/worktree.rs), renders progress (spinner/verbose), supports text and stream-json output, and classifies exit codes via src/exit_code.rs. Logging is configured to file-only with rotation in src/logger.rs. Chat Completions DTOs live in src/clients/chat_types.rs and are used by the client layer.

## Key Files
- src/main.rs — CLI flow: input assembly, model resolution, Agent wiring, sessions, progress/output, worktrees, exit handling.
- src/config/model.rs — ModelConfig, ApiType, defaults, and env-based API key resolution (ResolvedModelConfig).
- src/config/data_dir.rs — DataDir management, session save/load (flat files under `~/.local/share/cake/sessions/`), AGENTS.md discovery and loading.
- src/prompts/mod.rs — System prompt construction including AGENTS.md context, CWD, and date.
- src/exit_code.rs — Exit code constants and robust error classification (structured ApiError, reqwest, string patterns).
- src/clients/mod.rs — Client module aggregator; re-exports Agent and tool helpers; ties chat/responses APIs and tools.

## Conventions
- Sessions are JSONL at ~/.local/share/cake/sessions/{session_uuid}.jsonl; newest for a directory is by mtime after filtering by header working_directory.
- Worktrees live under <repo>/.cake/worktrees/<name> on branch worktree-<name>; removed if no changes.
- No logs to stderr/stdout; all tracing goes to rotating files in the cache dir (src/logger.rs); user output is strictly controlled by output_format/verbosity.
- CLI stdin handling: “-” requires stdin; otherwise stdin is read only if non-tty with a 100ms timeout and merged as “prompt\n\nstdin”.

## Gotchas
- README default model states glm-5; code defaults to glm-5.1 (src/config/defaults.rs and tests) — keep docs and code consistent.
- Stdin reader uses a 100ms timeout (src/main.rs::read_stdin_content); slow producers may be missed unless “-” is used.
- Saving a session requires a valid UUID; DataDir::save_session validates and errors on non-UUID IDs.

## Agent Instructions
- Always propagate errors with anyhow and map new API failures to src/exit_code.rs classification (use ApiError for HTTP status-aware cases).
- Always keep non-test code free of unwrap()/expect(); add tests for new behaviors similar to existing unit tests.
- Always route diagnostics through tracing (src/logger.rs); never write logs to stderr/stdout beyond the existing progress/output mechanisms.
- When adding CLI flags, define them on CodingAssistant (src/main.rs), respect quiet/verbose conflicts, implement behavior, and add parsing/unit tests.
- When modifying session logic, preserve JSONL format, flat file layout under `~/.local/share/cake/sessions/`, and saving on both success and error unless --no-session.
- When changing model defaults or API endpoints, update src/config/defaults.rs and synchronize README and tests.
- Ensure new client/tool functionality emits ConversationItem events compatible with format_progress_item/format_spinner_message and stream-json.
- Preserve worktree behavior: create under .cake/worktrees, branch worktree-<name>, cd into/restore dirs, and auto-remove only when clean.
