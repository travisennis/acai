# Session Management

Cake provides session persistence and restoration, enabling users to continue conversations across separate invocations. Sessions are tracked per project directory and saved with full metadata.

## Overview

Every time you run `cake`, a session is automatically created and saved. Sessions capture the full conversation history (messages, function calls, function outputs, reasoning) along with metadata such as timestamps and the working directory. Sessions are saved on both success and error, ensuring crash recovery.

## Usage

### Starting a Session

Every `cake` invocation creates a new session automatically:

```bash
cake "My favorite color is blue"
```

### Continuing the Latest Session

Use `--continue` to restore the most recent session for the current directory:

```bash
cake --continue "What's my favorite color?"
# The AI will remember "blue" from the previous session
```

### Resuming a Specific Session

Use `--resume <UUID>` to restore a specific session by its identifier:

```bash
cake --resume 550e8400-e29b-41d4-a716-446655440000 "Continue our conversation"
```

The UUID is scoped to the current directory — you can only resume sessions that were created in the same directory.

### Disabling Session Saving

Use `--no-session` to run a command without saving the session to disk:

```bash
cake --no-session "Quick one-off question"
```

This is useful for ephemeral queries where you don't need to continue the conversation later.

### Mutually Exclusive Flags

`--continue` and `--resume` cannot be used together. Cake will return an error if both are provided.

## How It Works

### Storage Layout

Sessions are stored under `~/.cache/cake/sessions/` organized by a hash of the working directory:

```
~/.cache/cake/sessions/
  {dir_hash}/
    {uuid}.jsonl        # Individual session files (JSONL format)
```

- **dir_hash**: First 16 hex characters of a SHA-256 hash of the absolute working directory path. This groups sessions by project directory.
- The most recent session is determined by file modification time (the newest `.jsonl` file).

### Session File Format

Sessions are stored in JSONL (JSON Lines) format, where each line is a separate JSON object. The first line is a session header, followed by one line per conversation item:

**Session header (first line):**
```json
{
  "format_version": 2,
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": "2024-03-04T10:30:00Z",
  "working_directory": "/Users/user/project",
  "model": "anthropic/claude-3.5-sonnet",
  "type": "session_start"
}
```

**Conversation items (subsequent lines):**
```json
{"format_version":2,"session_id":"550e8400-e29b-41d4-a716-446655440000","timestamp":"2024-03-04T10:30:05Z","working_directory":"/Users/user/project","type":"message","role":"user","content":"My favorite color is blue"}
{"format_version":2,"session_id":"550e8400-e29b-41d4-a716-446655440000","timestamp":"2024-03-04T10:30:10Z","working_directory":"/Users/user/project","type":"message","role":"assistant","content":"Got it! I'll remember that your favorite color is blue.","id":"msg_abc123","status":"completed"}
```

| Field | Type | Description |
|-------|------|-------------|
| `format_version` | number | Schema version for forward compatibility (currently `2`) |
| `session_id` | string | UUID v4 session identifier |
| `timestamp` | string | ISO 8601 timestamp when the line was written |
| `working_directory` | string | Absolute path of the directory where the session was created |
| `model` | string? | Optional model identifier used for the session |
| `type` | string | Line type: `session_start` for header, or `message`/`function_call`/`function_call_output`/`reasoning` for items |

**Note:** The system prompt is not stored in session files. It is built fresh from AGENTS.md files each time a session is started or restored. This ensures that any updates to the system prompt are always reflected.

### Message Types

Each line in the JSONL file (after the header) contains a typed conversation item:

- **`message`**: A text message with a role (`user`, `assistant`, `system`, `tool`)
- **`function_call`**: A tool invocation request from the model
- **`function_call_output`**: The result of a tool execution
- **`reasoning`**: Intermediate reasoning from models that support it

### Atomic Writes

Session files are written atomically to prevent corruption:

1. Session data is written to a temporary file (`{uuid}.tmp`)
2. The temporary file is renamed to the final path (`{uuid}.jsonl`)

### Save on Error

Sessions are saved regardless of whether the API call succeeds or fails. If an error occurs, the session file will contain all messages up to the point of failure. This enables recovery from crashes and network errors.

### Session Restoration

When restoring a session with `--continue` or `--resume`:

1. The session file is loaded from disk
2. A fresh `Responses` client is created with the current model and configuration
3. The client is configured with the restored session ID and conversation history
4. The new prompt is appended and sent to the API
5. The updated session (including the new exchange) is saved back to disk

Note: Only conversation history is restored. Model parameters (temperature, max tokens, etc.) are taken from the current invocation's flags.

## Directory Isolation

Sessions are isolated by working directory. Each directory gets its own namespace via the directory hash:

```bash
# Sessions in /Users/user/project-a are separate from /Users/user/project-b
cd /Users/user/project-a
cake "Working on project A"

cd /Users/user/project-b
cake --continue "What project am I working on?"
# Error: No previous session found for this directory
```

## Compatibility with Legacy History

The new session system coexists with the legacy timestamp-based history files in `~/.cache/cake/history/`. Legacy files are not migrated and remain available for audit purposes. New sessions are written exclusively to `~/.cache/cake/sessions/`.

## Implementation Details

- **Session struct**: `src/config/session.rs` (`Session`, `SessionLine`, `SessionHeader`)
- **Storage and retrieval**: `src/config/data_dir.rs` (`save_session`, `load_latest_session`, `load_session`)
- **CLI integration**: `src/main.rs` (`--continue`, `--resume`, `--fork`, `--no-session` flags)
- **Agent builder methods**: `src/clients/agent.rs` (`with_session_id`, `with_history`)
