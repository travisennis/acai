# `--streaming-json` Flag Documentation

The `--streaming-json` flag enables streaming JSON output for the `instruct` command. When enabled, each message, reasoning step, function call, and function call output is emitted as a separate JSON object to stdout in real-time as it's received from the API.

## Usage

```bash
acai instruct --streaming-json --prompt "Your prompt here"
```

Or with shorthand:

```bash
acai instruct --streaming-json -p "Your prompt here"
```

## Behavior

When `--streaming-json` is enabled:

1. **Init message** is emitted first with session ID, cwd, and tools
2. **System message** is streamed as a JSON object
3. **User message** is streamed as a JSON object  
4. **Assistant response** (including reasoning, function calls, and content) is streamed as JSON objects as they arrive from the API
5. **Result message** is emitted at the end with success/error status, duration, and usage stats
6. **No final output** is printed after completion (the streaming JSON is the only output)

When `--streaming-json` is disabled (default):

1. The final assistant response is printed to stdout as plain text
2. No intermediate JSON is emitted

## JSON Object Schema

The streaming output consists of four possible JSON object types, distinguished by the `type` field:

### 1. Message Object

Represents a conversation message from any role (system, user, assistant, tool).

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
| `type` | string | Always `"message"` for this type |
| `role` | string | One of: `"system"`, `"user"`, `"assistant"`, `"tool"` |
| `content` | string | The text content of the message |
| `id` | string? | Optional unique message identifier |
| `status` | string? | Optional message status (e.g., `"completed"`) |

### 2. Function Call Object

Represents a request to execute a function/tool.

```json
{
  "type": "function_call",
  "id": "fc_abc123",
  "call_id": "call_xyz789",
  "name": "Shell",
  "arguments": "{\"command\": \"ls -la\"}"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"function_call"` for this type |
| `id` | string | Unique identifier for the function call |
| `call_id` | string | Call identifier from the API |
| `name` | string | Name of the function being called |
| `arguments` | string | JSON string containing function arguments |

### 3. Function Call Output Object

Represents the result of a function execution.

```json
{
  "type": "function_call_output",
  "call_id": "call_xyz789",
  "output": "total 8\ndrwxr-xr-x  2 user  staff   64 Mar  1 10:00 .\nddrwxr-xr-x  1 user  staff  416 Feb 28 09:00 .."
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"function_call_output"` for this type |
| `call_id` | string | ID of the function call this output corresponds to |
| `output` | string | The result/output from the function execution |

### 4. Reasoning Object

Represents intermediate reasoning or thought process from the model (supported models only).

```json
{
  "type": "reasoning",
  "id": "reason_abc123",
  "summary": ["The user is asking about the weather, so I should use the weather tool..."]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"reasoning"` for this type |
| `id` | string | Unique identifier for the reasoning |
| `summary` | array | Array of reasoning text segments |

### 5. Init Object

Emitted at the start of a conversation when streaming is enabled. Contains session information, current working directory, and available tools.

```json
{
  "type": "init",
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "cwd": "/Users/user/project",
  "tools": ["shell"]
}
```

| Field | Type | Description |
|-------|------|-------------|
| `type` | string | Always `"init"` for this type |
| `session_id` | string | UUID for this conversation session |
| `cwd` | string | Current working directory when the command started |
| `tools` | array | Array of tool names (e.g., `["shell"]`) |

### 6. Result Object

Emitted at the end of a conversation when streaming is enabled. Contains success/error status, duration, and usage statistics.

**Success example:**
```json
{
  "type": "result",
  "success": true,
  "subtype": "success",
  "duration_ms": 1523,
  "turn_count": 2,
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
  "success": false,
  "subtype": "error",
  "error": "Error: API request failed: rate limit exceeded",
  "duration_ms": 342,
  "turn_count": 1,
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
| `type` | string | Always `"result"` for this type |
| `success` | boolean | Whether the request succeeded |
| `subtype` | string | One of: `"success"`, `"error"` |
| `error` | string? | Error message if `success` is false |
| `duration_ms` | number | Total duration in milliseconds |
| `turn_count` | number | Number of API calls made |
| `usage` | object | Token usage statistics |
| `usage.input_tokens` | number | Number of input tokens |
| `usage.input_tokens_details.cached_tokens` | number | Number of cached tokens |
| `usage.output_tokens` | number | Number of output tokens |
| `usage.output_tokens_details.reasoning_tokens` | number | Number of reasoning tokens |
| `usage.total_tokens` | number | Total tokens used |

## Example Output

Here's an example of the full streaming JSON output for a request that triggers a function call:

```json
{"type":"init","session_id":"550e8400-e29b-41d4-a716-446655440000","cwd":"/Users/user/project","tools":["shell"]}
{"type":"message","role":"system","content":"You are a helpful AI CLI assistant."}
{"type":"message","role":"user","content":"List files in the current directory"}
{"type":"reasoning","id":"reason_001","summary":["The user wants to list files. I'll use the Shell tool to run ls."]}
{"type":"function_call","id":"fc_001","call_id":"call_001","name":"Shell","arguments":"{\"command\":\"ls\"}"}
{"type":"message","role":"assistant","content":"Let me list the files for you."}
{"type":"function_call_output","call_id":"call_001","output":"file1.txt\nfile2.txt\nfile3.txt"}
{"type":"message","role":"assistant","content":"Here are the files in your current directory:\n- file1.txt\n- file2.txt\n- file3.txt"}
{"type":"result","success":true,"subtype":"success","duration_ms":1523,"turn_count":2,"usage":{"input_tokens":150,"input_tokens_details":{"cached_tokens":50},"output_tokens":320,"output_tokens_details":{"reasoning_tokens":120},"total_tokens":470}}
```

## Use Cases

- **JSON parsing**: Parse the output programmatically for integration with other tools
- **Real-time display**: Build custom UIs that show thinking/process in real-time
- **Debugging**: Inspect the exact structure of API responses
- **Function call handling**: Detect and handle function calls as they happen

## Notes

- Each JSON object is printed on its own line (newline-delimited JSON - NDJSON)
- The output is not guaranteed to be in any particular order beyond the temporal order of receipt
- Not all response types will appear in every request - only those returned by the model
- When using this flag, the final plain-text response is not printed (streaming JSON replaces it)
