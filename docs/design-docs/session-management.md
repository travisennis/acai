# Session Management

Cake provides session persistence and restoration, enabling users to continue conversations across separate invocations. Sessions are tracked per project directory and saved with full metadata.

## Overview

Every time you run `cake`, a session is automatically created and saved. Sessions capture the full conversation history (messages, function calls, function outputs, reasoning) along with metadata such as timestamps and the working directory. Sessions are saved on both success and error, ensuring crash recovery.

Sessions are stored in a unified JSONL format (v3) where every line is a `SessionRecord`. This is the same schema used by `--output-format stream-json`, so you can redirect stream-json output to a file and later resume from that file.

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

Use `--resume` to restore a session by UUID or file path:

```bash
# By UUID (loads from the sessions directory)
cake --resume 550e8400-e29b-41d4-a716-446655440000 "Continue our conversation"

# By file path (loads from an arbitrary file, e.g. redirected stream-json output)
cake --resume ./my_session.jsonl "Continue our conversation"
```

When resuming from a file path, cake checks that the current working directory matches the session's stored `working_directory`. If they don't match, cake exits with a clear error. Use UUID-based `--resume` to skip this check.

### Forking a Session

Use `--fork` to copy a session's history into a new session with a fresh ID:

```bash
# Fork the latest session for the current directory
cake --fork "Let's discuss something different"

# Fork a specific session by UUID
cake --fork 550e8400-e29b-41d4-a716-446655440000 "New branch of conversation"

# Fork from a file path
cake --fork ./my_session.jsonl "New approach"
```

### Disabling Session Saving

Use `--no-session` to run a command without saving the session to disk:

```bash
cake --no-session "Quick one-off question"
```

### Model Enforcement

When resuming or continuing a session, cake enforces model consistency. If the session was created with model X and you try to resume with model Y, cake will error out with a clear message. To continue with the session's model, use `--model X` or omit `--model` to automatically use the stored model.

### Mutually Exclusive Flags

`--continue` and `--resume` cannot be used together. Cake will return an error if both are provided.

## How It Works

### Storage Layout

Sessions are stored as flat files under `~/.local/share/cake/sessions/`:

```
~/.local/share/cake/sessions/
  {uuid}.jsonl        # Individual session files (JSONL format)
```

- Each session file is named with its UUID for easy reference.
- The most recent session for a directory is found by scanning all files, reading the `working_directory` field from each Init record, and picking the newest by file modification time.

### Session File Format (v3)

Sessions are stored in JSONL (JSON Lines) format using the unified `SessionRecord` schema. The first line is always an `Init` record, followed by zero or more conversation records, and optionally a `Result` record at the end for completed runs.

**Init record (first line):**
```json
{
  "type": "init",
  "format_version": 3,
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "timestamp": "2026-04-24T12:00:00Z",
  "working_directory": "/Users/user/project",
  "model": "anthropic/claude-3.5-sonnet",
  "tools": ["bash", "edit", "read", "write"]
}
```

**Conversation records (subsequent lines):**
```json
{"type":"message","role":"user","content":"My favorite color is blue"}
{"type":"message","role":"assistant","content":"Got it! I'll remember that your favorite color is blue.","id":"msg_abc123","status":"completed"}
{"type":"function_call","id":"fc_1","call_id":"call_1","name":"bash","arguments":"{\"command\":\"ls\"}"}
{"type":"function_call_output","call_id":"call_1","output":"file1.txt\nfile2.txt"}
{"type":"reasoning","id":"r_1","summary":["thinking..."],"encrypted_content":"gAAAAABencrypted..."}
```

**Result record (at end, for completed runs):**
```json
{"type":"result","subtype":"success","success":true,"is_error":false,"duration_ms":1523,"turn_count":2,"num_turns":2,"session_id":"550e8400-e29b-41d4-a716-446655440000","result":"Done!","usage":{"input_tokens":150,"input_tokens_details":{"cached_tokens":50},"output_tokens":320,"output_tokens_details":{"reasoning_tokens":120},"total_tokens":470}}
```

| Field | Type | Description |
|-------|------|-------------|
| `format_version` | number | Schema version for forward compatibility (currently `3`) |
| `session_id` | string | UUID v4 session identifier |
| `timestamp` | string | RFC3339 timestamp when the record was written |
| `working_directory` | string | Absolute path of the directory where the session was created |
| `model` | string? | Optional model identifier used for the session |
| `type` | string | Record type: `init`, `message`, `function_call`, `function_call_output`, `reasoning`, or `result` |

**Note:** The system prompt is not stored in session files. It is built fresh from AGENTS.md files each time a session is started or restored. This ensures that any updates to the system prompt are always reflected.

### Backward Compatibility

Existing v2 session files (which use `session_start` as the header type) continue to load. They are automatically migrated to v3 format on the next save. The v2 header is converted to an `Init` record, and v2 conversation lines are converted to `SessionRecord` variants.

### Save Semantics

Session files are no longer append-only. On save, the full file is rewritten to contain exactly:

1. One `Init` record at the top
2. Zero or more conversation records in the middle
3. At most one `Result` record at the end (for completed runs)

When resuming or forking, any trailing `Result` record from the previous run is stripped from the in-memory history, so the next save writes a fresh `Result`.

### Model Enforcement

When resuming or continuing a session, the stored model is compared against the resolved runtime model. If they differ, cake errors out with a clear message. If no `--model` is provided and the session has a stored model, cake uses that stored model as long as it is still configured in `settings.toml`. If the session has no stored model (old sessions), cake uses the configured `default_model`; if no `default_model` is configured, it exits with setup instructions.

### Atomic Writes

Session files are written atomically to prevent corruption:

1. Session data is written to a temporary file (`{uuid}.tmp`)
2. The temporary file is renamed to the final path (`{uuid}.jsonl`)

### Save on Error

Sessions are saved regardless of whether the API call succeeds or fails. If an error occurs, the session file will contain all messages up to the point of failure. This enables recovery from crashes and network errors.

### Session Restoration

When restoring a session with `--continue`, `--resume`, or `--fork`:

1. The session file is loaded from disk
2. A fresh agent is created with the current configuration
3. The agent is configured with the restored session ID and conversation history
4. The new prompt is appended and sent to the API
5. The updated session (including the new exchange) is saved back to disk

Note: Only conversation history is restored. Model parameters (temperature, max tokens, etc.) are taken from the current invocation's flags.

## Directory Isolation

Sessions are isolated by working directory. `--continue` and `--fork` (without an argument) only consider sessions whose Init record's `working_directory` matches the current directory:

```bash
# Sessions in /Users/user/project-a are separate from /Users/user/project-b
cd /Users/user/project-a
cake "Working on project A"

cd /Users/user/project-b
cake --continue "What project am I working on?"
# Error: No previous session found for this directory
```

## Migration from Old Storage

Earlier versions stored sessions under `~/.cache/cake/sessions/{dir_hash}/`. To migrate existing sessions to the new flat layout, run:

```bash
./migrate-sessions.sh
```

This moves all `{uuid}.jsonl` files from the old hash-based directories into `~/.local/share/cake/sessions/`. It is safe to run multiple times (skips files that already exist at the destination).

## Implementation Details

- **Session struct**: `src/config/session.rs` (`Session`, `SessionRecord`)
- **Storage and retrieval**: `src/config/data_dir.rs` (`save_session`, `load_latest_session`, `load_session`, `load_session_from_path`, `looks_like_uuid`)
- **CLI integration**: `src/main.rs` (`--continue`, `--resume`, `--fork`, `--no-session` flags)
- **Agent builder methods**: `src/clients/agent.rs` (`with_session_id`, `with_history`, `with_stream_records`, `drain_stream`, `emit_init_message`, `emit_result_message`)
- **Unified schema**: `src/clients/types.rs` (`SessionRecord`, `ResultSubtype`)
