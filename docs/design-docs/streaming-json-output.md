# `--output-format stream-json` Documentation

The `--output-format stream-json` option enables streaming JSON output for the main command. When enabled, each message, reasoning step, function call, and function call output is emitted as a separate JSON object to stdout in real-time as it's received from the API.

The stream-json output uses the same unified JSONL schema as session history files. This means you can redirect stream-json output to a file and later resume from that file using `--resume <path>`.

## Usage

```bash
cake --output-format stream-json "Your prompt here"

# Redirect to file for later resumption
cake --output-format stream-json "Your prompt here" > session.jsonl

# Resume from a saved stream-json file
cake --resume session.jsonl "Continue our conversation"
```

## Behavior

When `--output-format stream-json` is enabled:

1. **Init record** is emitted first with session ID, timestamp, working directory, model, and tools
2. **User message** is streamed as a JSON object
3. **Assistant response** (including reasoning, function calls, and content) is streamed as JSON objects as they arrive from the API
4. **Result record** is emitted at the end with success/error status, duration, and usage stats
5. **No final output** is printed after completion (the streaming JSON is the only output)

When `--output-format text` is used (default):

1. The final assistant response is printed to stdout as plain text
2. No intermediate JSON is emitted
3. Human-readable progress is streamed to stderr, and the final progress line includes the session ID, duration, turn count, and token usage

Note: The Result record is emitted and stored for both text and stream-json modes. Only the stdout emission of other records is conditional on `--output-format stream-json`.

## JSON Object Schema

Every line in the stream is a `SessionRecord` distinguished by the `type` field. The schema is versioned via `format_version` in the Init record (currently version 3).

### 1. Init Record

Emitted at the start of every conversation. Contains session metadata.

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

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"init"` |
| `format_version` | number | Schema version (currently `3`) |
| `session_id` | string | UUID for this conversation session |
| `timestamp` | string | RFC3339 timestamp of this snapshot |
| `working_directory` | string | Current working directory when the command started |
| `model` | string? | Model identifier (omitted if not available) |
| `tools` | array | Array of tool names |

### 2. Message Record

Represents a conversation message from any role (user, assistant, tool, system).

```json
{
  "type": "message",
  "role": "assistant",
  "content": "Hello! How can I help you today?",
  "id": "msg_abc123",
  "status": "completed"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"message"` |
| `role` | string | One of: `"user"`, `"assistant"`, `"tool"`, `"system"` |
| `content` | string | The text content of the message |
| `id` | string? | Optional unique message identifier |
| `status` | string? | Optional message status (e.g., `"completed"`) |
| `timestamp` | string? | Optional RFC3339 timestamp |

### 3. Function Call Record

Represents a request to execute a function/tool.

```json
{
  "type": "function_call",
  "id": "fc_abc123",
  "call_id": "call_xyz789",
  "name": "bash",
  "arguments": "{\"command\": \"ls -la\"}"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"function_call"` |
| `id` | string | Unique identifier for the function call |
| `call_id` | string | Call identifier from the API |
| `name` | string | Name of the function being called |
| `arguments` | string | JSON string containing function arguments |
| `timestamp` | string? | Optional RFC3339 timestamp |

### 4. Function Call Output Record

Represents the result of a function execution.

```json
{
  "type": "function_call_output",
  "call_id": "call_xyz789",
  "output": "total 8\ndrwxr-xr-x  2 user  staff   64 Mar  1 10:00 ."
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"function_call_output"` |
| `call_id` | string | ID of the function call this output corresponds to |
| `output` | string | The result/output from the function execution |
| `timestamp` | string? | Optional RFC3339 timestamp |

### 5. Reasoning Record

Represents intermediate reasoning or thought process from the model (supported models only). The `encrypted_content` and `content` fields are preserved for reasoning session roundtripping.

```json
{
  "type": "reasoning",
  "id": "reason_abc123",
  "summary": ["The user is asking about the weather, so I should use the weather tool..."],
  "encrypted_content": "gAAAAABencrypted...",
  "content": [{"type": "reasoning_text", "text": "deep thoughts"}]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"reasoning"` |
| `id` | string | Unique identifier for the reasoning |
| `summary` | array | Array of reasoning text segments |
| `encrypted_content` | string? | Opaque encrypted reasoning content for API roundtripping |
| `content` | array? | Original content array from the API response |
| `timestamp` | string? | Optional RFC3339 timestamp |

### 6. Result Record

Emitted at the end of a conversation. Contains success/error status, duration, and usage statistics.

**Success example:**
```json
{
  "type": "result",
  "subtype": "success",
  "success": true,
  "is_error": false,
  "duration_ms": 1523,
  "turn_count": 2,
  "num_turns": 2,
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "result": "Here are the files in your current directory...",
  "usage": {
    "input_tokens": 150,
    "input_tokens_details": {"cached_tokens": 50},
    "output_tokens": 320,
    "output_tokens_details": {"reasoning_tokens": 120},
    "total_tokens": 470
  }
}
```

**Error example:**
```json
{
  "type": "result",
  "subtype": "error_during_execution",
  "success": false,
  "is_error": true,
  "duration_ms": 342,
  "turn_count": 1,
  "num_turns": 1,
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "error": "API request failed: rate limit exceeded",
  "usage": {
    "input_tokens": 45,
    "input_tokens_details": {"cached_tokens": 0},
    "output_tokens": 0,
    "output_tokens_details": {"reasoning_tokens": 0},
    "total_tokens": 45
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"result"` |
| `subtype` | string | One of: `"success"`, `"error_during_execution"`, `"error_max_turns"` |
| `success` | boolean | Whether the request succeeded |
| `is_error` | boolean | Whether an error occurred (inverse of `success`) |
| `duration_ms` | number | Total duration in milliseconds |
| `turn_count` | number | Number of API calls made (kept for backward compat) |
| `num_turns` | number | Same as `turn_count`; alias for clarity |
| `session_id` | string | UUID of the session |
| `result` | string? | Final assistant message text on success |
| `error` | string? | Error message on failure |
| `usage` | object | Token usage statistics |
| `usage.input_tokens` | number | Number of input tokens |
| `usage.input_tokens_details.cached_tokens` | number | Number of cached tokens |
| `usage.output_tokens` | number | Number of output tokens |
| `usage.output_tokens_details.reasoning_tokens` | number | Number of reasoning tokens |
| `usage.total_tokens` | number | Total tokens used |
| `permission_denials` | array? | List of denied permission requests (only when present) |

## Example Output

Here's an example of the full streaming JSON output for a request that triggers a function call:

```json
{"type":"init","format_version":3,"session_id":"550e8400-e29b-41d4-a716-446655440000","timestamp":"2026-04-24T12:00:00Z","working_directory":"/Users/user/project","model":"anthropic/claude-3.5-sonnet","tools":["bash","edit","read","write"]}
{"type":"message","role":"user","content":"List files in the current directory"}
{"type":"reasoning","id":"reason_001","summary":["The user wants to list files. I'll use the bash tool to run ls."]}
{"type":"function_call","id":"fc_001","call_id":"call_001","name":"bash","arguments":"{\"command\":\"ls\"}"}
{"type":"message","role":"assistant","content":"Let me list the files for you."}
{"type":"function_call_output","call_id":"call_001","output":"file1.txt\nfile2.txt\nfile3.txt"}
{"type":"message","role":"assistant","content":"Here are the files in your current directory:\n- file1.txt\n- file2.txt\n- file3.txt"}
{"type":"result","subtype":"success","success":true,"is_error":false,"duration_ms":1523,"turn_count":2,"num_turns":2,"session_id":"550e8400-e29b-41d4-a716-446655440000","result":"Here are the files in your current directory:\n- file1.txt\n- file2.txt\n- file3.txt","usage":{"input_tokens":150,"input_tokens_details":{"cached_tokens":50},"output_tokens":320,"output_tokens_details":{"reasoning_tokens":120},"total_tokens":470}}
```

## Session Resumption from Stream-JSON Output

Since stream-json output uses the same schema as session files, you can redirect output to a file and later resume from it:

```bash
# Save a stream-json session to a file
cake --output-format stream-json "My prompt" > my_session.jsonl

# Resume from that file later
cake --resume my_session.jsonl "Continue the conversation"

# Fork from a saved session
cake --fork my_session.jsonl "Try a different approach"
```

When resuming from a file path (instead of a UUID), cake checks that the current working directory matches the session's `working_directory` field. If they don't match, cake exits with a clear error. Use a UUID-based `--resume` to skip this check.

## Use Cases

- **JSON parsing**: Parse the output programmatically for integration with other tools
- **Real-time display**: Build custom UIs that show thinking/process in real-time
- **Debugging**: Inspect the exact structure of API responses
- **Function call handling**: Detect and handle function calls as they happen
- **Session persistence**: Redirect output to a file for later resumption

## Notes

- Each JSON object is printed on its own line (newline-delimited JSON - NDJSON)
- The output is not guaranteed to be in any particular order beyond the temporal order of receipt
- Not all response types will appear in every request - only those returned by the model
- When using `--output-format stream-json`, the final plain-text response is not printed (streaming JSON replaces it)
- The `exit_code` field has been removed from the result record. The app still emits an exit code to the shell, but it is not persisted in the JSONL schema