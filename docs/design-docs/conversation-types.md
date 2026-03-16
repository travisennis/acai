# Conversation Types

The `clients::types` module defines the core data structures for representing conversations with AI models. These types map directly to the OpenRouter Responses API format.

## Overview

All conversation state flows through a single type: `ConversationItem`. This enum represents every possible item in a conversation:

- User messages
- Assistant messages
- Tool/function calls
- Tool outputs
- Reasoning traces (from reasoning models)

This design provides a **single source of truth** for conversation history, simplifying serialization, persistence, and API communication.

## ConversationItem

```rust
pub enum ConversationItem {
    Message { role, content, id, status },
    FunctionCall { id, call_id, name, arguments },
    FunctionCallOutput { call_id, output },
    Reasoning { id, summary, encrypted_content, content },
}
```

### Message

Represents a text message from any role:

```rust
ConversationItem::Message {
    role: Role,           // System, Assistant, User, or Tool
    content: String,      // The message text
    id: Option<String>,   // Required for assistant messages
    status: Option<String>, // "completed" or "incomplete"
}
```

The content format differs between API input and streaming output:
- **API input**: Structured as content arrays (`input_text` for user/system, `output_text` for assistant)
- **Streaming output**: Plain text for readability

### FunctionCall

Represents a request from the AI to execute a tool:

```rust
ConversationItem::FunctionCall {
    id: String,         // Unique call ID
    call_id: String,    // Reference ID for output
    name: String,       // Tool name (Bash, Read, Edit, Write)
    arguments: String,  // JSON arguments for the tool
}
```

### FunctionCallOutput

Represents the result of a tool execution:

```rust
ConversationItem::FunctionCallOutput {
    call_id: String,  // Matches the FunctionCall's call_id
    output: String,   // Tool result or error message
}
```

### Reasoning

Captures reasoning output from models like o1 or DeepSeek-R1:

```rust
ConversationItem::Reasoning {
    id: String,
    summary: Vec<String>,                    // Human-readable summary
    encrypted_content: Option<String>,       // Opaque encrypted content for round-tripping
    content: Option<Vec<ReasoningContent>>,  // Original content array for Chat Completions providers
}
```

The `encrypted_content` field preserves reasoning tokens that must be echoed back to the API for multi-turn conversations with reasoning models.

## Serialization

### to_api_input()

Converts a `ConversationItem` to the format required by the Responses API:

```rust
pub fn to_api_input(&self) -> serde_json::Value
```

Key transformations:
- Messages use `input_text`/`output_text` content arrays
- Reasoning summaries are wrapped in `summary_text` objects
- Assistant messages include `id` and `status` fields

### to_streaming_json()

Converts to a simplified format for `--output-format stream-json`:

```rust
pub fn to_streaming_json(&self) -> serde_json::Value
```

Key differences from `to_api_input`:
- Message content is plain text (not wrapped in objects)
- Reasoning summaries are plain strings (not objects)
- More compact for human consumption

## Usage Tracking

The module includes usage statistics types:

```rust
pub struct Usage {
    pub input_tokens: u32,
    pub input_tokens_details: InputTokensDetails,
    pub output_tokens: u32,
    pub output_tokens_details: OutputTokensDetails,
    pub total_tokens: u32,
}

pub struct InputTokensDetails {
    pub cached_tokens: u32,
}

pub struct OutputTokensDetails {
    pub reasoning_tokens: u32,
}
```

These track token consumption across the conversation, including cached tokens and reasoning tokens.

## Internal Types

The module also includes internal types for API request/response handling:

- **`Request`**: Struct for serializing API requests
- **`ApiResponse`**: Struct for deserializing API responses
- **`OutputMessage`**: Intermediate representation for parsing response items
- **`ProviderConfig`**: Configuration for provider restrictions

These are marked `pub(super)` as they are internal implementation details of the `clients` module.

## Design Decisions

### Single Enum vs. Multiple Types

Using a single `ConversationItem` enum rather than separate types for each item simplifies:

- **Collections**: `Vec<ConversationItem>` for history
- **Serialization**: One `#[serde(tag = "type")]` implementation
- **Pattern matching**: Exhaustive matching on all item types
- **Streaming**: Unified handling for all item types

### Content Arrays vs. Plain Text

The API uses content arrays for flexibility, but this adds complexity. The design:

- Stores plain text internally for simplicity
- Transforms to content arrays only when sending to API
- Keeps original content arrays for reasoning round-tripping

### Encrypted Content Preservation

Reasoning models return encrypted content that must be echoed back. The design:

- Stores encrypted content verbatim
- Skips serialization when `None` to reduce payload size
- Preserves content arrays for Chat Completions provider compatibility

## Testing

The module includes comprehensive tests for:

- Serialization round-trips for all item types
- API input format correctness
- Streaming JSON format
- Role-specific content handling
- Reasoning with/without encrypted content
- Usage statistics defaults

All tests use `#[allow(clippy::unwrap_used)]` as they are test code, not production.
