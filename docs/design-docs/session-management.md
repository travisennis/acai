# Session Management

Cake persists conversations as append-only JSONL session logs. A session is a long-lived conversation identified by UUID. A task is one CLI invocation within that session.

## Usage

New invocations create a new session automatically:

```bash
cake "My favorite color is blue"
```

`--continue` appends a new task to the most recent session for the current directory:

```bash
cake --continue "What's my favorite color?"
```

If the latest session exists but belongs to another directory, `--continue` fails with a directory mismatch instead of silently starting or selecting another session.

`--resume` accepts only a session UUID:

```bash
cake --resume 550e8400-e29b-41d4-a716-446655440000 "Continue our conversation"
```

`--fork` creates a new session UUID and seeds it with the parent conversation records. Use it without a value for the latest session in the current directory, or with a UUID for a specific parent:

```bash
cake --fork "Explore a different approach"
cake --fork 550e8400-e29b-41d4-a716-446655440000 "New branch"
```

Path-based `--resume <path>` and `--fork <path>` are not supported. Stream-json output is a live task feed, not a resumable session file.

## Storage Layout

Sessions are stored as flat files under `~/.local/share/cake/sessions/`:

```text
~/.local/share/cake/sessions/
  {uuid}.jsonl
```

`CAKE_DATA_DIR` overrides the data root and stores sessions under `{CAKE_DATA_DIR}/sessions/`.

## File Format

Session files use format version 4. The first line is exactly one `session_meta` record. Each invocation appends one `task_start`, the live conversation records for that task, and one `task_complete`.

```json
{"type":"session_meta","format_version":4,"session_id":"550e8400-e29b-41d4-a716-446655440000","timestamp":"2026-05-03T12:00:00Z","working_directory":"/Users/user/project","model":"anthropic/claude-3.5-sonnet","tools":["Bash","Read","Edit","Write"],"cake_version":"0.1.0"}
{"type":"task_start","session_id":"550e8400-e29b-41d4-a716-446655440000","task_id":"2b15f29d-8c42-4c53-9bdf-35c8f2390d3e","timestamp":"2026-05-03T12:00:01Z"}
{"type":"message","role":"user","content":"List files"}
{"type":"function_call","id":"fc_1","call_id":"call_1","name":"bash","arguments":"{\"command\":\"ls\"}"}
{"type":"function_call_output","call_id":"call_1","output":"Cargo.toml\nsrc"}
{"type":"message","role":"assistant","content":"The project contains Cargo.toml and src.","id":"msg_1","status":"completed"}
{"type":"task_complete","subtype":"success","success":true,"is_error":false,"duration_ms":1523,"turn_count":2,"num_turns":2,"session_id":"550e8400-e29b-41d4-a716-446655440000","task_id":"2b15f29d-8c42-4c53-9bdf-35c8f2390d3e","result":"The project contains Cargo.toml and src.","usage":{"input_tokens":150,"input_tokens_details":{"cached_tokens":50},"output_tokens":320,"output_tokens_details":{"reasoning_tokens":120},"total_tokens":470}}
```

Conversation records are `message`, `function_call`, `function_call_output`, and `reasoning`. Only those records are restored into model context. `session_meta`, `task_start`, and `task_complete` remain in the file but are skipped when reconstructing conversation history.

## Append Semantics

Session files are append-only. Cake does not rewrite or normalize previous records when continuing or resuming. Conversation records are appended live as the agent emits them, so a crash can still leave a partial task on disk. Loading tolerates a trailing `task_start` without a matching `task_complete`.

Cake takes an advisory exclusive lock on the session file for the duration of an invocation. A second writer receives:

```text
Another cake invocation is currently writing to session <id>. Wait for it to finish or run in a different directory.
```

## Compatibility

Version 4 is a breaking schema change. Legacy v2 files and old v3 files using `session_start`, `init`, or `result` do not load. A first record that is not `session_meta` is treated as a legacy or unsupported file. A `session_meta.format_version` other than `4` fails with an explicit unsupported-version error.

## Implementation Details

- Storage and loading: `src/config/session.rs`, `src/config/data_dir.rs`
- CLI orchestration: `src/main.rs`
- Agent task events and live persistence fan-out: `src/clients/agent.rs`
- Persisted and streamed schemas: `src/clients/types.rs` (`SessionRecord`, `StreamRecord`, `TaskCompleteSubtype`)
