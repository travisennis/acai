# Chat Completions API Support

Acai supports the Chat Completions API as an alternative backend for interacting with AI providers. This is the standard OpenAI-compatible format used by many providers.

## Overview

The Chat Completions API (`src/clients/chat_completions.rs`) provides a widely-supported interface for AI interactions. Acai's implementation:

- **Translates internal types**: Maps `ConversationItem` to Chat Completions message format
- **Groups tool calls**: Buffers consecutive function calls into a single assistant message
- **Handles streaming**: Supports real-time JSON streaming output
- **Omits reasoning**: Skips reasoning items (not supported by Chat Completions API)

## Architecture

### Backend Dispatch

The `Agent` struct dispatches to the Chat Completions backend based on `ApiType::ChatCompletions`:

```rust
match self.config.config.api_type {
    ApiType::Responses => responses::send_request(...),
    ApiType::ChatCompletions => chat_completions::send_request(...),
}
```

### Key Translations

The `build_messages()` function translates acai's internal `ConversationItem` history into Chat Completions messages:

| Internal Representation | Chat Completions Translation |
|---|---|
| `Role::System` | `"developer"` role |
| Consecutive `FunctionCall` items | Grouped into single assistant message with `tool_calls` array |
| `FunctionCallOutput` | `"tool"` role message with `tool_call_id` |
| `Reasoning` items | **Skipped** (not supported) |

### Request/Response Flow

1. **Build Messages**: Convert conversation history via `build_messages()`
2. **Convert Tools**: Transform internal tool definitions to Chat Completions format
3. **Send Request**: POST to configured base URL
4. **Parse Response**: Extract messages and tool calls from choices
5. **Execute Tools**: If tool calls present, execute and add results to history
6. **Loop**: Continue until no more tool calls

## Request Format

### ChatRequest

```rust
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_completion_tokens: Option<u32>,
    pub tools: Option<Vec<ChatTool>>,
    pub tool_choice: Option<String>,
}
```

### ChatMessage

```rust
pub struct ChatMessage {
    pub role: String,           // "developer", "assistant", "user", "tool"
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ChatToolCall>>,
    pub tool_call_id: Option<String>,
}
```

### ChatTool

```rust
pub struct ChatTool {
    pub type_: String,          // "function"
    pub function: ChatFunction,
}

pub struct ChatFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}
```

## Response Format

### ChatResponse

```rust
pub struct ChatResponse {
    pub id: Option<String>,
    pub choices: Vec<ChatChoice>,
    pub usage: Option<ChatUsage>,
}
```

### ChatChoice

```rust
pub struct ChatChoice {
    pub index: u32,
    pub message: ChatResponseMessage,
    pub finish_reason: Option<String>,
}
```

### ChatResponseMessage

```rust
pub struct ChatResponseMessage {
    pub role: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ChatToolCall>>,
}
```

### ChatUsage

```rust
pub struct ChatUsage {
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}
```

## Message Role Mapping

### System → "developer"

System messages are sent with the `"developer"` role instead of `"system"`, following the newer OpenAI convention for o1 models and newer.

### Assistant → "assistant"

Assistant messages and tool calls use the `"assistant"` role.

### User → "user"

User messages use the `"user"` role.

### FunctionCallOutput → "tool"

Tool results are sent with the `"tool"` role and include the `tool_call_id` to match them to their corresponding tool call.

## Tool Calling

### Consecutive FunctionCalls → Grouped tool_calls

When the model returns multiple function calls in a turn, they are stored as separate `ConversationItem::FunctionCall` entries internally. The Chat Completions backend buffers these and flushes them as a single assistant message with a `tool_calls` array:

```rust
// Internal representation (multiple items)
ConversationItem::FunctionCall { call_id: "call-1", name: "bash", ... }
ConversationItem::FunctionCall { call_id: "call-2", name: "read", ... }

// Chat Completions format (grouped)
{
  "role": "assistant",
  "content": null,
  "tool_calls": [
    { "id": "call-1", "type": "function", "function": { "name": "bash", ... } },
    { "id": "call-2", "type": "function", "function": { "name": "read", ... } }
  ]
}
```

### Tool Results

Tool results are sent as separate messages with the `"tool"` role:

```rust
{
  "role": "tool",
  "content": "output from tool",
  "tool_call_id": "call-1"
}
```

## Configuration

To use the Chat Completions API, configure `ApiType::ChatCompletions` in your `ModelConfig`:

```rust
ModelConfig {
    model: "glm-5".to_string(),
    api_type: ApiType::ChatCompletions,
    base_url: "https://opencode.ai/zen/go/v1".to_string(),
    api_key_env: "OPENCODE_ZEN_API_TOKEN".to_string(),
    temperature: Some(0.8),
    top_p: None,
    max_output_tokens: Some(8000),
    providers: vec![],
}
```

## Limitations

### No Reasoning Support

The Chat Completions API does not support reasoning items. Any `ConversationItem::Reasoning` items in the conversation history are silently skipped during serialization.

### Usage Tracking Differences

- **Cached tokens**: Not tracked by Chat Completions (always reported as 0)
- **Reasoning tokens**: Not tracked by Chat Completions (always reported as 0)

## CLI Usage

```bash
# Set API key (depends on your configured provider)
export OPENCODE_ZEN_API_TOKEN=your_key_here

# Basic usage - Chat Completions API is used when configured
./target/release/acai "Your prompt here"

# Streaming JSON output
./target/release/acai "Your prompt here" --output-format stream-json

# With max tokens override
./target/release/acai --max-tokens 4000 "Explain Rust ownership"
```

## OpenAI Chat Completions API Reference

For complete API documentation, see the [OpenAI Chat Completions API reference](https://platform.openai.com/docs/api-reference/chat/create).

### Key Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `model` | string | ID of the model to use |
| `messages` | array | List of messages comprising the conversation |
| `temperature` | number | Sampling temperature (0-2) |
| `top_p` | number | Nucleus sampling parameter |
| `max_completion_tokens` | number | Maximum tokens to generate |
| `tools` | array | List of tools the model may call |
| `tool_choice` | string/object | Controls which tool is called |

### Message Roles

| Role | Description |
|------|-------------|
| `developer` | Developer-provided instructions (replaces `system` for newer models) |
| `system` | System instructions (legacy, for older models) |
| `user` | End-user messages |
| `assistant` | Model responses |
| `tool` | Tool results (must include `tool_call_id`) |

### Finish Reasons

| Reason | Description |
|--------|-------------|
| `stop` | Natural stop point or stop sequence |
| `length` | Maximum token limit reached |
| `tool_calls` | Model called a tool |
| `content_filter` | Content omitted by filter |

## Examples

### Basic Chat

```json
{
  "model": "gpt-4o",
  "messages": [
    { "role": "developer", "content": "You are a helpful assistant." },
    { "role": "user", "content": "Hello!" }
  ]
}
```

### With Tool Calling

```json
{
  "model": "gpt-4o",
  "messages": [
    { "role": "user", "content": "What's the weather in Boston?" }
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "Get current weather",
        "parameters": {
          "type": "object",
          "properties": {
            "location": { "type": "string" }
          },
          "required": ["location"]
        }
      }
    }
  ],
  "tool_choice": "auto"
}
```

### Tool Result Follow-up

```json
{
  "model": "gpt-4o",
  "messages": [
    { "role": "user", "content": "What's the weather in Boston?" },
    {
      "role": "assistant",
      "content": null,
      "tool_calls": [
        {
          "id": "call_abc123",
          "type": "function",
          "function": { "name": "get_weather", "arguments": "{\"location\": \"Boston\"}" }
        }
      ]
    },
    {
      "role": "tool",
      "tool_call_id": "call_abc123",
      "content": "{\"temperature\": 72, \"condition\": \"sunny\"}"
    }
  ]
}
```

## References

- [OpenAI Chat Completions API Documentation](https://platform.openai.com/docs/guides/text-generation)
- [OpenAI Function Calling Guide](https://platform.openai.com/docs/guides/function-calling)
- [OpenAI API Reference](https://platform.openai.com/docs/api-reference/chat/create)
