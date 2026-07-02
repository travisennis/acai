---
status: accepted
date: 2026-06-14
---
# Model Request Timing Telemetry

## Context

Acai's logs expose token usage but not model API latency. There are no
`model.request.start`/`end` events, no time-to-first-token (TTFT), no per-token
throughput, no retry counts, no provider response headers, and no per-turn
wall-clock latency that separates model response time from tool execution time.
Without this instrumentation, every performance analysis is guesswork: inter-tool
gaps look like "model time" but network round-trip cannot be separated from output
decode.

The agent loop in `source/agent/index.ts` drives one model request per iteration
via the AI SDK `streamText`, then executes tools in parallel. This is the natural
place to measure timing because it already owns the request lifecycle and the tool
execution phase.

## Decision

Emit one structured log object per line (via the existing pino logger) for each
model request, correlated by a stable `requestId` of the form
`<sessionId>:<iteration>`:

- `model.request.start` — `event`, `requestId`, `sessionId`, `model`, `provider`,
  `iteration`, `inputTokenEstimate`.
- `model.first_token` — `event`, `requestId`, `ttftMs` (elapsed ms from request
  start to the first content-bearing stream chunk).
- `model.request.end` — `event`, `requestId`, `modelResponseMs`, `outputTokens`,
  `outputTokensPerSecond`, `reasoningTokens`, `inputTokens`, `finishReason`,
  `retryCount`, `providerRequestId`, and selected provider response headers
  (`x-request-id` and any `ratelimit`/`x-ratelimit`/`retry-after` headers).

Each log line carries an `event` field naming the event so existing line-oriented
log tooling can filter without parsing message text.

Persist per-turn timing in the session via a `SessionManager` timing accumulator
that mirrors the token-usage shape (`total` plus `lastTurn`), recording
`wallClockMs`, `modelMs`, and `toolMs` per agent iteration. The session summary
surfaces total wall-clock ms, model response ms, aggregate tool execution ms,
non-tool overhead ms, and a session-level "tool time vs total time" rollup.

The timing boundaries within each iteration are:

- `requestStart` immediately before `streamText`.
- `modelStreamEnd` immediately after the `fullStream` loop completes.
- tool phase measured around parallel tool execution.
- `modelMs = modelStreamEnd - requestStart`, `toolMs` = tool phase wall clock,
  `wallClockMs` = full iteration, `overheadMs = wallClockMs - modelMs - toolMs`.

## Rationale

- The agent loop already owns both the model stream and the tool execution phase,
  so it can cleanly separate model time from tool time without new plumbing.
- Reusing pino keeps events as one JSON object per line, so existing log tooling
  stays simple.
- Mirroring the existing `SessionTokenUsage` `total`/`lastTurn` shape keeps the
  session file format familiar and the accumulation logic consistent.
- `requestId = sessionId:iteration` is stable, monotonic, and joinable across the
  three events without depending on provider-supplied ids that may be absent.

## Consequences

- **Positive**: Model latency, TTFT, throughput, retries, and the tool-vs-model
  time split become observable from logs and session summaries.
- **Positive**: Future work (e.g., tool-call size logging, parallelism benchmarks)
  can join on `requestId`.
- **Negative**: Adds a new persisted `timing` field to session files; older
  sessions without it must be treated as optional/absent.
- **Negative**: Provider header capture is best-effort and varies per provider.

## Alternatives Considered

- **Wrap the model in middleware**: A `wrapLanguageModel` middleware could emit
  timing, but it cannot see the tool execution phase that the agent loop owns, so
  the model-vs-tool split would still require loop-level instrumentation.
- **Per-step usage only**: Reusing the existing token-usage path alone cannot
  express TTFT or the model-vs-tool wall-clock split.

## References

- Task `.agents/.tasks/completed/001.md`
- `source/agent/index.ts`, `source/sessions/manager.ts`, `source/sessions/summary.ts`
- ADR 008: Token Tracking Strategy

