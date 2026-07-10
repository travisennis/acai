---
status: accepted
date: 2026-07-02
decision-makers: Travis Ennis
---
# Multi-provider Fallback Strategy

## Context

The acai CLI tool needs to handle failures gracefully when AI providers experience issues. Users should not be blocked by temporary outages, and the system should provide appropriate fallback behavior.

## Decision

### Retry Strategy

The agent implements retry logic for transient failures:

```typescript
const maxRetries = args.maxRetries ?? 2;

// In error handling:
if (consecutiveErrors > maxRetries) {
  yield {
    type: "agent-error",
    message: `Exceeded maximum retry attempts (${maxRetries}).`,
  };
  break;
}
```

### AI SDK Built-in Retries

The AI SDK handles connection-level retries:

```typescript
const result = streamText({
  model: langModel,
  maxRetries: 2,  // SDK-level retries
  // ...
});
```

### Invalid Tool Input Recovery

Invalid tool inputs from the model are not fatal errors:

```typescript
if (InvalidToolInputError.isInstance(error)) {
  yield {
    type: "agent-error",
    message: `Tool input validation failed: ${error.message}. Try again with valid arguments.`,
  };
  // Continue loop to allow user to provide corrected input
}
```

### Error Context Logging

Errors include rich context for debugging:

```typescript
const errorContext = {
  modelId: modelConfig?.id ?? langModel.modelId ?? "unknown",
  sessionId: sessionManager.getSessionId(),
  messageCount: sessionManager.get().length,
  attempt: consecutiveErrors,
  maxRetries,
  responseStatus: cause.response?.status,
  responseBody: cause.response?.body,
};

logger.error({ ...errorContext, error: err }, `Error on streamText...`);
```

### Rate Limiting

Middleware enforces rate limits:

```typescript
createRateLimitMiddleware({ requestsPerMinute: 30 })
```

### Model Selection

Users select models explicitly via the model command, allowing manual fallback:

```bash
/model anthropic:claude-sonnet-4-20250514
```

The ModelManager emits events when models change:

```typescript
export class ModelManager extends EventEmitter<ModelManagerEvents> {
  private modelMap: Map<App, LanguageModelV3>;
  emit("set-model", app: App, model: ModelName);
}
```

### No Automatic Provider Switching

Currently, there is no automatic provider fallback (e.g., switching from Anthropic to OpenAI on failure). This design choice reflects:
- Different provider APIs may have subtly different behavior
- Model context window sizes differ
- Cost structures differ
- Users may have API keys for only some providers

Automatic switching could be added as a future enhancement.

## Consequences

### Positive
- Retries handle transient network issues
- Invalid input recovery prevents fatal errors
- Rich error context aids debugging
- Rate limiting prevents quota exhaustion
- Manual model switching gives user control

### Negative
- No automatic cross-provider fallback
- Retries add latency on persistent failures
- Error recovery messages can be confusing to users

### Future Enhancements

**Provider Groups:** Define groups of equivalent models (e.g., "claude-equivalent") that can fall back to each other:
```typescript
const providerGroups = [
  ["anthropic:claude-sonnet-4", "openai:gpt-4o", "google:gemini-2-flash"],
];
```

**Circuit Breaker:** Track failure rates per provider and temporarily skip unhealthy providers.

