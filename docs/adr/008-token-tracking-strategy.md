---
status: accepted
date: 2026-07-02
decision-makers: Travis Ennis
---
# Token Tracking Strategy

## Decision

### Tracking Scope

Token tracking aggregates usage across all model calls within an application:

```typescript
export class TokenTracker {
  private counters: Map<string, ModelUsage>;
  trackUsage(app: string, usage: LanguageModelUsage): void;
  getTotal(app: string): ModelUsage;
  getGrandTotal(): ModelUsage;
}
```

### Per-App Tracking

Different application contexts track tokens separately:

```typescript
// In ModelManager.setModel:
tokenTracker.trackUsage("repl", stepUsage);

// In title generation:
tokenTracker.trackUsage("title-conversation", result.usage);

// In tool repair:
tokenTracker.trackUsage("tool-repair", result.usage);
```

### Usage Data Structure

```typescript
type ModelUsage = {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  cachedInputTokens: number;
  reasoningTokens: number;
  inputTokenDetails: {
    noCacheTokens: number;
    cacheReadTokens: number;
    cacheWriteTokens: number;
  };
  outputTokenDetails: {
    textTokens: number;
    reasoningTokens: number;
  };
};
```

### Cost Estimation

Token tracker maintains running cost estimates using model metadata:

```typescript
recordTurnUsage(usage: TurnUsage): void {
  const modelConfig = this.modelManager.getModelMetadata("repl");
  const estimatedCost =
    usage.inputTokens * modelConfig.costPerInputToken +
    usage.outputTokens * modelConfig.costPerOutputToken;

  this.tokenUsage.total.estimatedCost += estimatedCost;
}
```

### Context Window Management

Session manager tracks context window usage to detect near-capacity situations:

```typescript
setContextWindow(contextWindow: number): void;
getContextWindow(): number;
getLastTurnContextWindow(): number {
  return this.tokenUsage?.lastTurn.totalTokens ?? 0;
}
```

### Reporting

The footer component displays token usage:

```typescript
class Footer implements Component {
  render(width: number): string[] {
    const usage = tokenTracker.getTotal("repl");
    return [
      `Tokens: ${usage.totalTokens.toLocaleString()}`,
      `Cost: $${usage.estimatedCost.toFixed(4)}`,
    ];
  }
}
```

### No Server-Side Estimation

Tokens are counted from actual model responses, not estimated. The AI SDK returns accurate usage data from provider APIs.

### Counters

Simple in-memory counters track per-session totals:

```typescript
private counters: Map<string, ModelUsage> = new Map();

trackUsage(app: string, usage: LanguageModelUsage): void {
  const existing = this.counters.get(app);
  if (existing) {
    existing.totalTokens += usage.totalTokens ?? 0;
    // ...
  } else {
    this.counters.set(app, { ...usage });
  }
}
```

## Consequences

### Positive
- Accurate per-call tracking from provider responses
- Cost estimation enables budget awareness
- Per-app breakdown for debugging
- Context window tracking prevents overflow

### Negative
- No persistence across sessions
- No budget limits or hard stops
- Cost estimates use static pricing, not actual bills

### Future Enhancements

**Budget Limits:** Enforce per-session or per-day spending limits with configurable actions (warn, block, switch model).

**Historical Tracking:** Persist usage to a database for analytics and reporting.

**Dynamic Pricing:** Fetch current pricing from provider APIs or user-configured overrides.

