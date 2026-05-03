# Responses API Support

Cake integrates with OpenRouter's Responses API to provide multi-provider AI access with support for reasoning, tool calling, and structured conversation management.

## Overview

The Responses API is OpenRouter's unified interface for interacting with various AI providers. Cake implements a client (`src/clients/responses.rs`) that handles:

- **Conversation Management**: Typed conversation items (messages, function calls, function outputs, reasoning)
- **Tool Calling**: Built-in Bash tool execution with configurable timeouts
- **Reasoning Support**: Captures and streams reasoning tokens from models that support them
- **Usage Tracking**: Accumulates token usage statistics across multiple API turns
- **Streaming Output**: Optional JSON streaming for real-time message delivery

## Architecture

### Conversation Items

The Responses API uses a structured input/output format where each conversation turn is represented as a typed item. Cake models this with the `ConversationItem` enum:

```rust
pub enum ConversationItem {
    Message {
        role: Role,
        content: String,
        id: Option<String>,      // Required for assistant messages
        status: Option<String>,  // "completed" or "incomplete"
    },
    FunctionCall {
        id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
    },
    Reasoning {
        id: String,
        summary: Vec<String>,
    },
}
```

### Message Roles

Messages use a `Role` enum to distinguish senders:

```rust
pub enum Role {
    System,     // System instructions
    Assistant,  // AI responses
    User,       // User inputs
    Tool,       // Tool results
}
```

### Request/Response Flow

1. **Build Input**: Convert conversation history to Responses API format via `build_input()`
2. **Send Request**: POST to `https://openrouter.ai/api/v1/responses`
3. **Parse Output**: Extract all output items (reasoning, function calls, messages)
4. **Execute Tools**: If function calls present, execute them and add results to history
5. **Loop**: Continue until no more function calls are returned

## Features

### Tool Calling

Cake includes a built-in `Bash` tool for executing shell commands:

```rust
fn bash_tool() -> Tool {
    Tool {
        type_: "function".to_string(),
        name: "Bash".to_string(),
        description: "Execute a shell command in the host machine's terminal...",
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The shell command to execute" },
                "timeout": { "type": "number", "description": "Optional timeout in seconds" }
            },
            "required": ["command"]
        }),
    }
}
```

Custom tools can be added via `with_tools()`.

### Reasoning Support

Models that support reasoning (e.g., DeepSeek R1, MiniMax M2.5) return reasoning items that are captured and stored:

```rust
ConversationItem::Reasoning {
    id: id.clone(),
    summary: vec![text],
}
```

### Usage Tracking

The client accumulates usage statistics across all API calls:

```rust
pub struct Usage {
    pub input_tokens: u32,
    pub input_tokens_details: InputTokensDetails,  // includes cached_tokens
    pub output_tokens: u32,
    pub output_tokens_details: OutputTokensDetails,  // includes reasoning_tokens
    pub total_tokens: u32,
}
```

### Streaming JSON Output

Enable streaming mode to receive JSON messages in real-time:

```rust
let client = Responses::new(model, system_prompt)
    .with_streaming_json(|json| {
        println!("{}", json);
    });
```

Streamed messages include:
- `task_start`: Session and task ids for the current invocation
- `message`: User and assistant messages
- `function_call`: Tool invocations
- `function_call_output`: Tool results
- `reasoning`: Model reasoning
- `task_complete`: Final status with duration and usage stats

## CLI Usage

```bash
# Set API key
export OPENROUTER_API_KEY=your_key_here

# Basic usage (text output)
./target/release/cake "Your prompt here"

# Streaming JSON output
./target/release/cake "Your prompt here" --output-format stream-json

# With options
./target/release/cake \
    --model "minimax/minimax-m2.5" \
    --max-tokens 4000 \
    --prompt "Explain Rust ownership"
```

## Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `--model` | (from ModelConfig) | AI model to use |
| `--max-tokens` | `8000` | Maximum output tokens |
| `--output-format` | `text` | Output format (`text` or `stream-json`) |

> **Note:** Model, temperature, top-p, api_type, and base URL are configured per-model in `ModelConfig` (`src/config/model.rs`), not via CLI flags.

## Implementation Details

### Input Format

Messages are converted to the Responses API input format:

```json
{
  "type": "message",
  "role": "user",
  "content": [{ "type": "input_text", "text": "..." }]
}
```

For assistant messages, the output format is used:

```json
{
  "type": "message",
  "role": "assistant",
  "content": [{ "type": "output_text", "text": "...", "annotations": [] }],
  "id": "...",
  "status": "completed"
}
```

### Agent Loop

The client implements an agent loop that:

1. Sends the conversation history to the API
2. Receives output items (reasoning, function calls, messages)
3. If function calls are present, executes them and adds results to history
4. Loops back to step 1 until no more function calls
5. Returns the final assistant message

This enables multi-turn tool interactions without manual intervention.

## References

### Docs

- https://openrouter.ai/docs/api/reference/responses/overview.md
- https://openrouter.ai/docs/api/reference/responses/basic-usage.md
- https://openrouter.ai/docs/api/reference/responses/reasoning.md
- https://openrouter.ai/docs/api/reference/responses/tool-calling.md
- https://openrouter.ai/docs/api/reference/responses/error-handling.md
- https://openrouter.ai/docs/api/api-reference/responses/create-responses.md

### OpenResponses OpenAPI Schema

- https://www.openresponses.org/openapi/openapi.json
