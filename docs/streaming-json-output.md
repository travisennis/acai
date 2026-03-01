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

1. **System message** is streamed first as a JSON object
2. **User message** is streamed as a JSON object  
3. **Assistant response** (including reasoning, function calls, and content) is streamed as JSON objects as they arrive from the API
4. **No final output** is printed after completion (the streaming JSON is the only output)

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

## Example Output

Here's an example of the full streaming JSON output for a request that triggers a function call:

```json
{"type":"message","role":"system","content":"You are a helpful AI CLI assistant."}
{"type":"message","role":"user","content":"List files in the current directory"}
{"type":"reasoning","id":"reason_001","summary":["The user wants to list files. I'll use the Shell tool to run ls."]}
{"type":"function_call","id":"fc_001","call_id":"call_001","name":"Shell","arguments":"{\"command\":\"ls\"}"}
{"type":"message","role":"assistant","content":"Let me list the files for you."}
{"type":"function_call_output","call_id":"call_001","output":"file1.txt\nfile2.txt\nfile3.txt"}
{"type":"message","role":"assistant","content":"Here are the files in your current directory:\n- file1.txt\n- file2.txt\n- file3.txt"}
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
