# `--output-format stream-json`

`--output-format stream-json` emits newline-delimited JSON records for the current task as they happen. It is a live progress feed, not a complete session file.

## Usage

```bash
cake --output-format stream-json "Your prompt here"
```

The final plain-text assistant response is not printed in this mode. Consumers should read the streamed `message` and `task_complete` records.

## Behavior

The stream starts with `task_start`, includes the current task's conversation records, and ends with `task_complete`.

It never emits `session_meta`. It also does not replay prior tasks when used with `--continue`, `--resume`, or `--fork`.

Redirected stream-json output cannot be resumed by path. Use the persisted session UUID with `--resume <uuid>`.

## Records

### Task Start

```json
{"type":"task_start","session_id":"550e8400-e29b-41d4-a716-446655440000","task_id":"2b15f29d-8c42-4c53-9bdf-35c8f2390d3e","timestamp":"2026-05-03T12:00:01Z"}
```

### Message

```json
{"type":"message","role":"assistant","content":"Hello!","id":"msg_abc123","status":"completed"}
```

Roles are `system`, `user`, `assistant`, or `tool`. `id`, `status`, and `timestamp` are optional.

### Function Call

```json
{"type":"function_call","id":"fc_abc123","call_id":"call_xyz789","name":"bash","arguments":"{\"command\":\"ls\"}"}
```

### Function Call Output

```json
{"type":"function_call_output","call_id":"call_xyz789","output":"Cargo.toml\nsrc"}
```

### Reasoning

```json
{"type":"reasoning","id":"reason_abc123","summary":["Inspecting the project"],"encrypted_content":"gAAAAABencrypted..."}
```

`encrypted_content` and `content` are optional and are preserved for reasoning model round-tripping.

### Task Complete

```json
{"type":"task_complete","subtype":"success","success":true,"is_error":false,"duration_ms":1523,"turn_count":2,"num_turns":2,"session_id":"550e8400-e29b-41d4-a716-446655440000","task_id":"2b15f29d-8c42-4c53-9bdf-35c8f2390d3e","result":"Done","usage":{"input_tokens":150,"input_tokens_details":{"cached_tokens":50},"output_tokens":320,"output_tokens_details":{"reasoning_tokens":120},"total_tokens":470}}
```

`subtype` is one of `success`, `error_during_execution`, or `error_max_turns`. On failure, `error` contains the error message and `result` is omitted.

## Example

```json
{"type":"task_start","session_id":"550e8400-e29b-41d4-a716-446655440000","task_id":"2b15f29d-8c42-4c53-9bdf-35c8f2390d3e","timestamp":"2026-05-03T12:00:01Z"}
{"type":"message","role":"user","content":"List files in the current directory"}
{"type":"function_call","id":"fc_001","call_id":"call_001","name":"bash","arguments":"{\"command\":\"ls\"}"}
{"type":"function_call_output","call_id":"call_001","output":"Cargo.toml\nsrc"}
{"type":"message","role":"assistant","content":"The current directory contains Cargo.toml and src.","id":"msg_001","status":"completed"}
{"type":"task_complete","subtype":"success","success":true,"is_error":false,"duration_ms":1523,"turn_count":2,"num_turns":2,"session_id":"550e8400-e29b-41d4-a716-446655440000","task_id":"2b15f29d-8c42-4c53-9bdf-35c8f2390d3e","result":"The current directory contains Cargo.toml and src.","usage":{"input_tokens":150,"input_tokens_details":{"cached_tokens":50},"output_tokens":320,"output_tokens_details":{"reasoning_tokens":120},"total_tokens":470}}
```
