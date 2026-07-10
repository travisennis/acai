---
status: accepted
date: 2026-07-02
decision-makers: Travis Ennis
---
# Session Persistence Format

## Context

The acai CLI tool needs to persist conversation state between sessions for resumption. Sessions should be storable in JSON format with metadata, message history, and token usage tracking.

## Decision

### Session File Format

Sessions are stored as JSON files in `~/.acai/sessions/`:

```json
{
  "project": "my-project",
  "sessionId": "uuid-v4",
  "modelId": "claude-sonnet-4-20250514",
  "title": "Generated conversation title",
  "createdAt": "2026-04-15T10:30:00.000Z",
  "updatedAt": "2026-04-15T11:45:00.000Z",
  "messages": [
    { "role": "user", "content": [...] },
    { "role": "assistant", "content": [...] },
    { "role": "tool", "content": [...] }
  ],
  "tokenUsage": {
    "total": { "inputTokens": 1000, "outputTokens": 500, ... },
    "lastTurn": { "inputTokens": 100, "outputTokens": 50, ... }
  },
  "metadata": {}
}
```

### Token Usage Format

To minimize file size, token usage is stored in compact aggregated format rather than per-turn:

```typescript
type SessionTokenUsage = {
  total: {
    inputTokens: number;
    outputTokens: number;
    totalTokens: number;
    cachedInputTokens: number;
    reasoningTokens: number;
    estimatedCost: number;
  };
  lastTurn: {
    inputTokens: number;
    outputTokens: number;
    totalTokens: number;
    cachedInputTokens: number;
    reasoningTokens: number;
    estimatedCost: number;
  };
};
```

### Message Format

Messages follow AI SDK's `ModelMessage` format:

```typescript
type ModelMessage =
  | UserModelMessage      // { role: "user", content: [...] }
  | AssistantModelMessage // { role: "assistant", content: [...] }
  | ToolModelMessage;    // { role: "tool", content: [...] }
```

Content is an array of parts for flexibility:
```typescript
type TextPart = { type: "text"; text: string };
type ImagePart = { type: "image"; image: Uint8Array | URL | string };
type ToolCallPart = { type: "tool-call"; toolName: string; toolCallId: string; input: unknown };
type ToolResultPart = { type: "tool-result"; toolName: string; toolCallId: string; output: unknown };
```

### Session Manager

```typescript
export class SessionManager extends EventEmitter {
  // State
  private history: ModelMessage[];
  private sessionId: string;
  private title: string;
  private tokenUsage: SessionTokenUsage | null;

  // Methods
  appendUserMessage(msg: string | UserModelMessage): void;
  appendAssistantMessage(msg: string | AssistantModelMessage): void;
  appendToolMessages(messages: ToolModelMessage[]): void;
  appendResponseMessages(messages: ResponseMessage[]): void;
  sanitizeResponseMessages(messages: ResponseMessage[]): ResponseMessage[];

  // Persistence
  save(): Promise<void>;    // Atomic write via temp file
  restore(history: SavedMessageHistory): void;

  // Static
  static load(stateDir: string, count?: number): Promise<SavedMessageHistory[]>;
}
```

### Save Strategy

Session saves are atomic to prevent corruption:
1. Write to `session-UUID.json.tmp`
2. Rename to `session-UUID.json`

The SessionManager is called from interrupt handlers, so save failures are logged but not thrown.

### Title Generation

The first user message triggers title generation via a separate model call:
```typescript
async generateTitle(message: string): Promise<void>
```
Title generation failures fall back to truncating the first message.

### Deduplication

Tool-result messages are deduplicated on append to handle cases where both the AI SDK and acai generate results:
```typescript
appendToolMessages(toolResultMessages: ToolModelMessage[]) {
  // Filter out duplicates based on toolCallId
}
```

### Sanitization

Assistant messages with malformed tool call JSON are sanitized before storage to prevent history corruption:
```typescript
sanitizeResponseMessages(messages: ResponseMessage[]): ResponseMessage[]
```

## Consequences

### Positive
- JSON format is human-readable and debuggable
- Atomic writes prevent session corruption
- Compact token usage reduces file size
- Backward compatibility with older formats via migration
- AI SDK message format allows for future extensibility

### Negative
- Large sessions can grow large; no built-in pruning
- Image parts increase file size significantly
- No encryption at rest

### Alternatives Considered

**Binary Format:** MessagePack or Protocol Buffers would be smaller and faster, but JSON is human-readable and easier to debug. Rejected for simplicity.

**Per-turn Token Storage:** Storing every turn's usage would be useful for detailed analytics but increases file size significantly. Current compact format is sufficient for current needs.

