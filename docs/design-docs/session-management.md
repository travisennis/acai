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

Session files use format version 4. They are newline-delimited JSON. Each line is one `SessionRecord` serialized with a top-level `type` discriminator.

The first non-empty line must be exactly one `session_meta` record. Every saved invocation appends one `task_start`, the live conversation records emitted during that invocation, and one `task_complete`.

```json
{"type":"session_meta","format_version":4,"session_id":"550e8400-e29b-41d4-a716-446655440000","timestamp":"2026-05-03T12:00:00Z","working_directory":"/Users/user/project","model":"anthropic/claude-3.5-sonnet","tools":["bash","read","edit","write"],"cake_version":"0.1.0"}
{"type":"task_start","session_id":"550e8400-e29b-41d4-a716-446655440000","task_id":"2b15f29d-8c42-4c53-9bdf-35c8f2390d3e","timestamp":"2026-05-03T12:00:01Z"}
{"type":"prompt_context","session_id":"550e8400-e29b-41d4-a716-446655440000","task_id":"task-1","role":"developer","content":"Current working directory: /work\nToday's date: 2026-05-03","timestamp":"2026-05-03T12:00:00Z"}
{"type":"message","role":"user","content":"List files"}
{"type":"function_call","id":"fc_1","call_id":"call_1","name":"bash","arguments":"{\"command\":\"ls\"}"}
{"type":"function_call_output","call_id":"call_1","output":"Cargo.toml\nsrc"}
{"type":"message","role":"assistant","content":"The project contains Cargo.toml and src.","id":"msg_1","status":"completed"}
{"type":"task_complete","subtype":"success","success":true,"is_error":false,"duration_ms":1523,"turn_count":2,"num_turns":2,"session_id":"550e8400-e29b-41d4-a716-446655440000","task_id":"2b15f29d-8c42-4c53-9bdf-35c8f2390d3e","result":"The project contains Cargo.toml and src.","usage":{"input_tokens":150,"input_tokens_details":{"cached_tokens":50},"output_tokens":320,"output_tokens_details":{"reasoning_tokens":120},"total_tokens":470}}
```

Conversation records are `message`, `function_call`, `function_call_output`, and `reasoning`. Only those records are restored into model context. `session_meta`, `task_start`, `prompt_context`, and `task_complete` remain in the file but are skipped when reconstructing conversation history.

`prompt_context` records are append-only audit records for the mutable context used by a single invocation, such as AGENTS.md contents, discovered skills, and environment details. On continue, resume, or fork, cake rebuilds fresh prompt context and appends new `prompt_context` records; it does not replay stale prompt context from earlier invocations.

## Schema

All timestamps are UTC RFC 3339 strings. `session_id` and `task_id` are UUID strings. Optional fields are omitted when absent.

### `session_meta`

`session_meta` appears once, as the first record in the file.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Always `session_meta` |
| `format_version` | number | yes | Current value is `4` |
| `session_id` | string | yes | Stable session UUID; also used as the `{uuid}.jsonl` filename |
| `timestamp` | string | yes | Session creation time |
| `working_directory` | string | yes | Directory where the session was created |
| `model` | string | no | Resolved model identifier used for the session |
| `tools` | array of strings | yes | Enabled tool names at session creation |
| `cake_version` | string | no | Package version that created the file |
| `system_prompt` | string | no | Stable system prompt used when the session was created |
| `git` | object | yes | Git repository state at session creation |

The `git` object contains `repository_url`, `branch`, and `commit_hash`. Each
property is `null` when cake cannot determine that value, such as outside a git
repository or in a detached HEAD state.

### `task_start`

`task_start` marks one CLI invocation inside the session.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Always `task_start` |
| `session_id` | string | yes | Session UUID |
| `task_id` | string | yes | UUID for this invocation |
| `timestamp` | string | yes | Task start time |

### `prompt_context`

`prompt_context` records the mutable prompt context used for one CLI invocation.
These records are session-file audit entries and are not restored as conversation
history.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Always `prompt_context` |
| `session_id` | string | yes | Session UUID |
| `task_id` | string | yes | UUID for this invocation |
| `role` | string | yes | Logical role for this context, currently `developer` |
| `content` | string | yes | Context content sent to the model for this invocation |
| `timestamp` | string | yes | Record creation time |

### `message`

`message` stores user, assistant, or tool text. Older sessions may contain
system or developer messages; current prompt context is stored separately in
`session_meta.system_prompt` and `prompt_context`.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Always `message` |
| `role` | string | yes | One of `user`, `assistant`, or `tool` for current sessions |
| `content` | string | yes | Plain text message content |
| `id` | string | no | Provider message id, normally present for assistant messages from the Responses API |
| `status` | string | no | Provider status such as `completed` or `incomplete` |
| `timestamp` | string | no | Item creation time |

### `function_call`

`function_call` stores a model request to execute a tool.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Always `function_call` |
| `id` | string | yes | Provider function-call item id |
| `call_id` | string | yes | Correlation id used by the matching output |
| `name` | string | yes | Tool name, for example `bash`, `read`, `edit`, or `write` |
| `arguments` | string | yes | JSON-encoded tool argument string exactly as received from the model |
| `timestamp` | string | no | Item creation time |

### `function_call_output`

`function_call_output` stores the result of a tool execution.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Always `function_call_output` |
| `call_id` | string | yes | Matches the preceding `function_call.call_id` |
| `output` | string | yes | Tool output or tool error text returned to the model |
| `timestamp` | string | no | Item creation time |

### `reasoning`

`reasoning` preserves reasoning-model output needed for future API turns.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Always `reasoning` |
| `id` | string | yes | Provider reasoning item id |
| `summary` | array of strings | yes | Human-readable reasoning summaries |
| `encrypted_content` | string | no | Opaque provider content that must be echoed back for some reasoning models |
| `content` | array of objects | no | Provider reasoning content array, preserved for round-tripping |
| `timestamp` | string | no | Item creation time |

Each `content` item has:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Provider content item type, for example `reasoning_text` |
| `text` | string | no | Text for content item types that carry text |

### `task_complete`

`task_complete` records the outcome and aggregate usage for one invocation.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `type` | string | yes | Always `task_complete` |
| `subtype` | string | yes | One of `success`, `error_during_execution`, or `error_max_turns` |
| `success` | boolean | yes | `true` for successful completion |
| `is_error` | boolean | yes | Inverse of `success` in current code |
| `duration_ms` | number | yes | Wall-clock task duration in milliseconds |
| `turn_count` | number | yes | Number of API turns with usage accumulated |
| `num_turns` | number | yes | Alias of `turn_count` retained for consumer compatibility |
| `session_id` | string | yes | Session UUID |
| `task_id` | string | yes | Task UUID from the matching `task_start` |
| `result` | string | no | Final assistant text on success |
| `error` | string | no | Error message on failure |
| `usage` | object | yes | Aggregate token usage for the task |
| `permission_denials` | array of strings | no | Tool permission denial messages when present |

`usage` has this shape:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `input_tokens` | number | yes | Total input tokens |
| `input_tokens_details.cached_tokens` | number | yes | Cached input tokens |
| `output_tokens` | number | yes | Total output tokens |
| `output_tokens_details.reasoning_tokens` | number | yes | Reasoning output tokens |
| `total_tokens` | number | yes | Provider-reported total tokens |

## Append Semantics

Session files are append-only. Cake does not rewrite or normalize previous records when continuing or resuming. Conversation records are appended live as the agent emits them, so a crash can still leave a partial task on disk. Loading tolerates a trailing `task_start` without a matching `task_complete`.

Cake takes an advisory exclusive lock on the session file for the duration of an invocation. A second writer receives:

```text
Another cake invocation is currently writing to session <id>. Wait for it to finish or run in a different directory.
```

## Compatibility

Version 4 is a breaking schema change. Legacy v2 files and old v3 files using `session_start`, `init`, or `result` do not load. A first record that is not `session_meta` is treated as a legacy or unsupported file. A `session_meta.format_version` other than `4` fails with an explicit unsupported-version error.

Forking creates a new v4 session file with a new `session_meta`. It seeds only conversation records from the parent; parent `session_meta`, `task_start`, and `task_complete` records are not copied.

Stream-json output uses the same task and conversation record shapes as v4 session files, but omits `session_meta` and includes only the current task. A redirected stream-json file is not a valid session file and cannot be resumed by path.

## Implementation Details

- Storage and loading: `src/config/session.rs`, `src/config/data_dir.rs`
- CLI orchestration: `src/main.rs`
- Agent task events and live persistence fan-out: `src/clients/agent.rs`
- Persisted and streamed schemas: `src/clients/types.rs` (`SessionRecord`, `StreamRecord`, `TaskCompleteSubtype`)
