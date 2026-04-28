# API Retry Strategy

cake talks to OpenAI-compatible Responses API and Chat Completions API providers. Retry behavior must therefore prefer portable HTTP semantics over provider-specific error shapes.

## Goals

- Recover from short-lived provider failures without user intervention.
- Keep retries bounded and visible in text mode.
- Fail fast when the provider indicates the request should not be retried.
- Avoid encoding Anthropic-only assumptions as universal API behavior.

## Retry Decision Order

The retry classifier applies decisions in this order:

1. Stop when the retry budget is exhausted.
2. For a parseable context-window overflow `400`, retry once with a lower output-token budget.
3. Stop when `x-should-retry: false` is present.
4. Retry portable transient HTTP statuses: `408`, `409`, `429`, `500`, `502`, `503`, and `504`.
5. Retry recognized provider-specific transient signals when no explicit no-retry header is present, including vendor `529` and `overloaded_error` bodies.
6. Treat `x-should-retry: true` as an extra retry signal for otherwise borderline `5xx` responses.
7. Stop for everything else, including ordinary `400`, `401`, `403`, and `404`.

An explicit `x-should-retry: false` header is authoritative. If it conflicts with a provider-specific body marker such as `overloaded_error`, cake honors the header and does not retry. This is the safer default for a generic compatible-provider client because the header is a direct provider decision while body markers may come from one provider family.

## Delay Selection

`Retry-After` wins when present and parseable. The parser supports delta seconds and HTTP-date values.

When no `Retry-After` is available, cake uses bounded exponential backoff with deterministic jitter derived from the session id and attempt number. The jitter de-synchronizes concurrent sessions without adding a random-number dependency.

## Transport Recovery

Retryable transport failures include timeouts, connection failures, connection resets, broken pipes, and unexpected EOF markers in the error chain. When such a failure is detected, cake retries the current turn with a client that disables idle connection reuse, then restores the default client after a successful turn.

## Context-Overflow Recovery

For parseable context-window overflow errors, cake computes:

```text
available_output = context_limit - input_tokens - 1024
```

If `available_output` is at least `256`, cake retries once with a lower `max_output_tokens` value. For Responses API requests with a configured reasoning budget, it also lowers `reasoning.max_tokens` to fit within the available output budget.

This retry is one-shot per turn. If the retry still fails, cake returns the provider error.

## Verification

Retry behavior should be verified with a policy matrix instead of relying on live providers to emit rare failures. Unit tests cover the pure classifier, and mock HTTP tests cover request bodies, retry callbacks, attempt counts, and final errors.

Important matrix cases:

| Status | Body/Header Signal | Expected |
|--------|--------------------|----------|
| `401` | any | no retry |
| `403` | any | no retry |
| `400` | ordinary invalid request | no retry |
| `400` | parseable context overflow | retry once with lower token budget |
| `429` | `Retry-After` | retry using the header delay |
| `503` | `x-should-retry: false` | no retry |
| `503` | `overloaded_error` + `x-should-retry: false` | no retry |
| `503` | `overloaded_error` | retry |
| `529` | no no-retry header | retry |
| `500` | `x-should-retry: true` | retry |
| `404` | `x-should-retry: true` | no retry |

Live provider checks should be limited to deterministic cases such as fake-key `401` and malformed-request `400`. Captured real provider error responses can be saved as fixtures and fed into the classifier when adding provider-specific behavior.
