# Acai Architecture

Acai is an AI coding assistant CLI that integrates with language models via the OpenRouter API. It provides a conversation-based interface with tool execution capabilities for file manipulation, code editing, and shell command execution.

## Overview

The core problem acai solves: provide a safe, sandboxed environment for AI agents to interact with the local filesystem and execute commands, while maintaining conversation context across sessions.

Key design decisions:
- **Agent loop**: The model can request tool executions; results are fed back, and the loop continues until the model returns a final response
- **Conversation as data**: All interaction history is represented as typed `ConversationItem` values
- **Session persistence**: Conversations can be saved, resumed, continued, or forked
- **OS-level sandboxing**: Bash commands run within platform-specific sandboxes (Seatbelt on macOS, Landlock on Linux)
- **Strict layering**: Dependencies flow in one direction only, preventing circular imports

## Codemap

### Layer 4: CLI (`cli` module)

Entry point for user interaction. Parses command-line arguments using clap, validates flag combinations, and dispatches to command implementations.

Key types: `CmdRunner` trait (the interface all commands implement), `instruct::Cmd` (the main command).

The CLI layer is intentionally thin—it delegates all business logic to lower layers.

### Layer 3: Clients (`clients` module)

The bridge to external AI services and the orchestration layer for tool execution.

**`agent`**: The `Agent` struct orchestrates the conversation loop, tool execution, streaming, and retry logic. It dispatches API calls to backends based on `ApiType` (Responses or Chat Completions). This is the public-facing type (`clients::Agent`).

**`responses`**: Backend for the Responses API. Provides `send_request()` and `parse_response()` functions that handle the Responses API wire format. No longer contains orchestration logic.

**`chat_completions`**: Backend for the Chat Completions API. Provides `send_request()` and `parse_response()` functions. Translates `ConversationItem` to/from chat completions format: groups consecutive `FunctionCall` items into assistant messages with `tool_calls`, maps `System` role to `"developer"`, and skips `Reasoning` items.

**`chat_types`**: Request/response DTOs for the Chat Completions API (`ChatRequest`, `ChatResponse`, `ChatMessage`, `ChatTool`, `ChatToolCall`, etc.).

**`types`**: Core conversation abstraction. The `ConversationItem` enum represents all possible items in a conversation: user messages, assistant messages, tool calls, tool outputs, and reasoning traces. This is the fundamental data structure that flows through the entire system.

**`tools`**: Tool definitions and execution. Each tool (Bash, Read, Edit, Write) defines its JSON schema for the API and its execution logic. The `execute_tool` function dispatches to the appropriate implementation. Tools validate that paths are within the working directory or allowed temp directories before operating.

**`tools::sandbox`**: Cross-platform sandboxing abstraction. Provides `SandboxConfig` and `SandboxStrategy` for restricting filesystem and network access. Platform-specific implementations use sandbox-exec (macOS) or Landlock LSM (Linux).

### Layer 2: Config, Models, Prompts

Foundation modules that provide data persistence, core types, and prompt generation.

**`config`**:
- `DataDir`: Manages the `~/.cache/acai/` directory, session storage, and AGENTS.md discovery
- `Session`: In-memory session state with JSONL serialization
- `worktree`: Git worktree utilities for isolated execution environments
- `model`: Contains `ApiType` enum (`Responses`/`ChatCompletions`), `ModelConfig` struct (model, api_type, base_url, api_key_env, temperature, top_p, max_output_tokens, reasoning_effort, reasoning_summary, reasoning_max_tokens, providers), and `ResolvedModelConfig` (resolves API key from env var)
- `settings`: TOML-based configuration loading from `settings.toml` files. Supports loading from both project-level (`.acai/settings.toml`) and global (`~/.cache/acai/settings.toml`) with merge semantics where project settings override global settings for the same model name.
- `defaults`: Default values for model, base URL, API key env var, and providers

Sessions are stored in a directory hashed from the working directory path (SHA-256, first 16 hex chars), ensuring isolation between projects.

**`models`**:
- `Message`: Simple role+content struct for high-level API
- `Role`: Enum of System, Assistant, User, Tool

These types are intentionally simple—most of the system uses `ConversationItem` directly.

**`prompts`**: System prompt construction with AGENTS.md integration. Reads both user-level (`~/.acai/AGENTS.md`) and project-level (`./AGENTS.md`) instruction files and injects them into the system prompt.

### Layer 1: Foundation

**`logger`**: File-only logging using tracing with daily rotation and 7-day retention. Defaults to INFO level, with debug/trace available via `RUST_LOG` environment variable.

External crates: `anyhow` for error handling, `tokio` for async runtime, `serde` for serialization, `reqwest` for HTTP.

## Architectural Invariants

These constraints guide the design and are unlikely to change:

1. **Dependencies flow downward only**: `cli` → `clients` → `config/models/prompts`. Cross-layer imports are prohibited (e.g., `config` cannot import from `clients`).

2. **All internal imports use absolute paths**: `crate::module::Item`, never relative paths like `super::` or `self::`.

3. **ConversationItem is the single source of truth**: All conversation state flows through this enum. There is no parallel representation.

4. **Tools validate paths before execution**: Every filesystem operation checks that the target path is within the working directory or an allowed temp directory (`/tmp`, `/var/folders`, etc.).

5. **Session writes are atomic**: Sessions are written to a temp file, then renamed to the final path. The "latest" session is tracked via a symlink that is also updated atomically.

6. **Sandboxing is opt-out, not opt-in**: Sandboxing applies to all bash commands unless explicitly disabled via `ACAI_SANDBOX=0`.

7. **No unwrap/expect in production code**: The clippy configuration denies these, enforced at compile time.

## System Boundaries

### CLI ↔ Client Boundary

The CLI layer owns argument parsing and user-facing error messages. The client layer owns all network communication and tool execution. The boundary is the `CmdRunner` trait: `async fn run(&self, data_dir: &DataDir) -> anyhow::Result<()>`. The CLI creates an `Agent` via `ModelConfig::default()` → `ResolvedModelConfig::resolve()` → `Agent::new()`.

### Client ↔ Tool Boundary

Tools are pure functions from JSON arguments to `ToolResult`. The client owns the orchestration (concurrent execution, timeout handling, result aggregation). Tools own the validation and side effects.

### Config ↔ Filesystem Boundary

All filesystem access for configuration and sessions goes through `DataDir`. No other module constructs paths into `~/.cache/acai/` directly.

### Host ↔ Sandbox Boundary

The sandbox configuration defines a strict boundary between what the host process can access and what the sandboxed subprocess can access. Read-only paths, read-write paths, and executable paths are explicitly enumerated.

## Cross-Cutting Concerns

### Error Handling

- Library code uses `thiserror` for typed errors
- Application code uses `anyhow` for context propagation
- Tool errors are stringified and returned to the model as function call output (the model can decide how to proceed)

### Logging

- File appender with daily rotation: `~/.cache/acai/acai.YYYY-MM-DD.log`
- Maximum 7 files retained (older files automatically deleted)
- Default level: INFO (debug/trace require `RUST_LOG=acai=debug` or `RUST_LOG=acai=trace`)
- Pattern includes timestamps, levels, file:line for debugging
- Non-blocking writes (async-safe)

### Session Management

Sessions enable conversation persistence across separate acai invocations:

- **New**: Generate UUID, create fresh session
- **Continue**: Load the most recent session for the working directory, append new messages
- **Resume**: Load a specific session by UUID, continue from there
- **Fork**: Copy history from existing session, generate new UUID (diverges without affecting original)

Storage format is JSONL (JSON Lines): each line is a complete JSON object, allowing append-only writes and partial recovery.

### Streaming JSON Output

When `--output-format stream-json` is specified, the system emits machine-readable JSON events:
- `ConversationItem` values as they are created
- Final usage statistics
- Error events

Logging is automatically suppressed to avoid polluting stdout.

## Finding Things

Use symbol search to locate specific implementations:

- **Agent loop**: Search for `Agent` struct and its `send` method in `agent.rs`
- **Tool execution**: Search for `execute_tool` function in `tools/mod.rs`
- **API dispatch**: Search for `complete_turn` method in `agent.rs`
- **Chat Completions translation**: Search for `build_messages` in `chat_completions.rs`
- **Model configuration**: Search for `ModelConfig` in `config/model.rs`
- **Session loading**: Search for `Session::load` or `DataDir::load_latest_session`
- **Path validation**: Search for `validate_path_in_cwd`
- **Sandbox profiles**: Search for `SandboxConfig` and platform-specific implementations
- **Conversation types**: Search for `ConversationItem` enum definition

## Reading List

For deeper understanding of specific subsystems:

- `docs/design-docs/cli.md` - Command-line interface and command dispatch
- `docs/design-docs/conversation-types.md` - ConversationItem enum and API types
- `docs/design-docs/models.md` - Role and Message types
- `docs/design-docs/prompts.md` - System prompt construction and AGENTS.md integration
- `docs/design-docs/session-management.md` - Session lifecycle and storage format
- `docs/design-docs/sandbox.md` - Sandbox implementation details
- `docs/design-docs/streaming-json-output.md` - Machine-readable output format
- `docs/design-docs/logging.md` - Logging configuration and troubleshooting
- `docs/design-docs/tools.md` - Tool framework (Bash, Read, Edit, Write)
- `docs/references/responses-api.md` - OpenRouter Responses API integration
