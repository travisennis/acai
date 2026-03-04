# Session Management

Acai provides session persistence and restoration, enabling users to continue conversations across separate invocations. Sessions are tracked per project directory and saved with full metadata.

## Overview

Every time you run `acai instruct`, a session is automatically created and saved. Sessions capture the full conversation history (messages, function calls, function outputs, reasoning) along with metadata such as timestamps and the working directory. Sessions are saved on both success and error, ensuring crash recovery.

## Usage

### Starting a Session

Every `acai instruct` invocation creates a new session automatically:

```bash
acai instruct --prompt "My favorite color is blue"
```

### Continuing the Latest Session

Use `--continue` to restore the most recent session for the current directory:

```bash
acai instruct --continue --prompt "What's my favorite color?"
# The AI will remember "blue" from the previous session
```

### Resuming a Specific Session

Use `--resume <UUID>` to restore a specific session by its identifier:

```bash
acai instruct --resume 550e8400-e29b-41d4-a716-446655440000 --prompt "Continue our conversation"
```

The UUID is scoped to the current directory — you can only resume sessions that were created in the same directory.

### Disabling Session Saving

Use `--no-session` to run a command without saving the session to disk:

```bash
acai instruct --no-session --prompt "Quick one-off question"
```

This is useful for ephemeral queries where you don't need to continue the conversation later.

### Mutually Exclusive Flags

`--continue` and `--resume` cannot be used together. Acai will return an error if both are provided.

## How It Works

### Storage Layout

Sessions are stored under `~/.cache/acai/sessions/` organized by a hash of the working directory:

```
~/.cache/acai/sessions/
  {dir_hash}/
    {uuid}.json          # Individual session files
    latest -> {uuid}.json  # Symlink to the most recent session
```

- **dir_hash**: First 16 hex characters of a SHA-256 hash of the absolute working directory path. This groups sessions by project directory.
- **latest**: A symlink that always points to the most recent session file, updated atomically on each save.

### Session File Format

Each session is saved as a JSON file with the following structure:

```json
{
  "format_version": 1,
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "working_dir": "/Users/user/project",
  "system_prompt": "You are a helpful AI CLI assistant...",
  "created_at": 1709500000000,
  "updated_at": 1709500030000,
  "messages": [
    {
      "type": "message",
      "role": "system",
      "content": "You are a helpful AI CLI assistant...",
      "id": null,
      "status": null
    },
    {
      "type": "message",
      "role": "user",
      "content": "My favorite color is blue",
      "id": null,
      "status": null
    },
    {
      "type": "message",
      "role": "assistant",
      "content": "Got it! I'll remember that your favorite color is blue.",
      "id": "msg_abc123",
      "status": "completed"
    }
  ]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `format_version` | number | Schema version for forward compatibility (currently `1`) |
| `id` | string | UUID v4 session identifier |
| `working_dir` | string | Absolute path of the directory where the session was created |
| `system_prompt` | string | System prompt used for this session |
| `created_at` | number | Unix timestamp in milliseconds when the session was created |
| `updated_at` | number | Unix timestamp in milliseconds when the session was last updated |
| `messages` | array | Conversation history as typed items (see [Responses API docs](responses-api.md)) |

### Message Types

The `messages` array contains typed conversation items:

- **`message`**: A text message with a role (`system`, `user`, `assistant`, `tool`)
- **`function_call`**: A tool invocation request from the model
- **`function_call_output`**: The result of a tool execution
- **`reasoning`**: Intermediate reasoning from models that support it

### Atomic Writes

Session files are written atomically to prevent corruption:

1. Session data is written to a temporary file (`{uuid}.tmp`)
2. The temporary file is renamed to the final path (`{uuid}.json`)
3. The `latest` symlink is updated via a temporary symlink + rename

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
acai instruct --prompt "Working on project A"

cd /Users/user/project-b
acai instruct --continue --prompt "What project am I working on?"
# Error: No previous session found for this directory
```

## Compatibility with Legacy History

The new session system coexists with the legacy timestamp-based history files in `~/.cache/acai/history/`. Legacy files are not migrated and remain available for audit purposes. New sessions are written exclusively to `~/.cache/acai/sessions/`.

## Implementation Details

- **Session struct**: `src/config/session.rs`
- **Storage and retrieval**: `src/config/data_dir.rs` (`save_session`, `load_latest_session`, `load_session`)
- **CLI integration**: `src/cli/cmds/instruct.rs` (`--continue`, `--resume`, `--no-session` flags)
- **Client builder methods**: `src/clients/responses.rs` (`with_session_id`, `with_history`)
