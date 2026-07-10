---
status: accepted
date: 2026-07-02
decision-makers: Travis Ennis
---
# Use AI SDK for Model Abstraction

## Context

The acai CLI tool needs to support multiple AI model providers (Anthropic, OpenAI, Google, Groq, DeepSeek, OpenRouter, xAI, OpenCode) with a unified interface. Each provider has different APIs, authentication methods, and response formats. We need to abstract these differences while preserving provider-specific capabilities.

## Decision

We use the Vercel AI SDK (`ai` package) as the core abstraction layer for model interactions, combined with provider-specific implementations.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        acai                               │
├─────────────────────────────────────────────────────────────┤
│  Agent (streamText, generateText)                           │
│  ModelManager (per-app model instances, metadata)           │
├─────────────────────────────────────────────────────────────┤
│  AI SDK Core (`ai` package)                                 │
│  - Provider Registry (createProviderRegistry)               │
│  - LanguageModel interface                                  │
│  - Middleware (cache, rate-limit, audit)                   │
├─────────────────────────────────────────────────────────────┤
│  Provider Implementations                                    │
│  ┌──────────────┬──────────────┬──────────────┐           │
│  │  anthropic    │   openai     │   google     │  ...      │
│  └──────────────┴──────────────┴──────────────┘           │
└─────────────────────────────────────────────────────────────┘
```

### Provider Registry Pattern

Each provider exports:
- Provider instance (e.g., `anthropicProvider`)
- Model registry (e.g., `anthropicModelRegistry`)
- Model names array (e.g., `anthropicModelNames`)

```typescript
// providers.ts
const registry = createProviderRegistry({
  ...anthropicProvider,
  ...openaiProvider,
  ...googleProvider,
  // ...
});

export function languageModel(model: ModelName) {
  return registry.languageModel(model);
}
```

### Custom Metadata Layer

The AI SDK's LanguageModel interface is extended with our own metadata:

```typescript
export interface ModelMetadata<T = ModelName> {
  id: T;
  provider: ModelProvider;
  contextWindow: number;
  supportsToolCalling: boolean;
  supportsReasoning: boolean;
  costPerInputToken: number;
  costPerOutputToken: number;
  maxOutputTokens: number;
  defaultTemperature: number;
  promptFormat: "xml" | "markdown" | "bracket";
}
```

### Middleware Pipeline

Tools wrap the base language model with middleware for cross-cutting concerns:

```typescript
const langModel = wrapLanguageModel({
  model: languageModel(model),
  middleware: [
    cacheMiddleware,
    createRateLimitMiddleware({ requestsPerMinute: 30 }),
    auditMessage({ filePath: stateDir, app }),
  ],
});
```

### ModelManager

Per-application model instances with metadata caching:

```typescript
export class ModelManager extends EventEmitter<ModelManagerEvents> {
  private modelMap: Map<App, LanguageModelV3>;
  private modelMetadataMap: Map<App, ModelMetadata>;

  setModel(app: App, model: ModelName);
  getModel(app: App): LanguageModelV3;
  getModelMetadata(app: App): ModelMetadata;
}
```

## Consequences

### Positive
- Unified interface across all providers
- Middleware pipeline for consistent cross-cutting concerns
- Built-in streaming, tool calling, and retry handling
- Active maintenance by Vercel with broad provider support
- Automatic schema generation for tool definitions via Zod

### Negative
- Dependency on AI SDK version and its provider support timeline
- Some provider-specific features may require workarounds
- Memory overhead from middleware chain on every request

### Alternatives Considered

**Custom Abstraction Layer:** Building our own provider interface from scratch would be significantly more work and harder to maintain. Rejected in favor of leveraging existing battle-tested code.

**Direct Provider SDKs:** Using Anthropic SDK, OpenAI SDK, etc. directly would tie us to multiple SDKs with different interfaces and require manual normalization. Rejected due to maintenance burden.

**LangChain:** While powerful, LangChain is heavier and more complex than needed for this use case. The AI SDK provides a lighter-weight solution. Rejected for simplicity.

