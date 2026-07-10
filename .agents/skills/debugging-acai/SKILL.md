---
name: debugging-acai
description: Debugging tools and techniques for acai. Use when investigating errors, analyzing session behavior, or troubleshooting issues in the acai CLI application. Includes guidance on reading logs, session files, and common error patterns.
---

# Debugging acai

## Key Principles

1. **Never read log files directly** - Use `tail` to view the end of log files
2. **Use dynamic-read-session** to inspect sessions - Don't read session JSON files directly
3. **Correlate errors with sessions** - Use sessionId from error logs to find relevant sessions

## Log Files

### Application Logs

Location: `~/.acai/logs/current.log`

**Reading logs:**
```bash
# View last 50 lines
tail -n 50 ~/.acai/logs/current.log

# Follow logs in real-time (in tmux)
tail -f ~/.acai/logs/current.log

# Search for specific session errors
grep "sessionId" ~/.acai/logs/current.log | tail -20
```

### Common Log Patterns

- **ERROR level**: Application errors that may need investigation
- **WARN level**: Non-fatal issues that might affect behavior
- **agent-error events**: Errors emitted by the agent loop

### Model Request Timing Telemetry

The agent loop emits one structured JSON log line per model request, correlated
by a stable `requestId` of the form `<sessionId>:<iteration>`. Each request emits
three events (filter on the `event` field):

- `model.request.start` — `model`, `provider`, `iteration`, `inputTokenEstimate`
- `model.first_token` — `ttftMs` (time-to-first-token in ms)
- `model.request.end` — `modelResponseMs`, `ttftMs`, `inputTokens`,
  `outputTokens`, `outputTokensPerSecond`, `reasoningTokens`, `finishReason`,
  `retryCount`, `providerRequestId`, and selected `providerHeaders`
  (request id, rate-limit, retry-after)

```bash
# All timing events for a session
grep '"requestId":"<sessionId>:' ~/.acai/logs/current.log

# Just request-end metrics
grep '"event":"model.request.end"' ~/.acai/logs/current.log | tail -20
```

Per-turn timing (`wallClockMs`, `modelMs`, `toolMs`) is also persisted in the
session file and rolled up in the exit Session Summary (Total/Model/Tools/
Overhead and Tool/Total ratio), so you can see whether a session is dominated by
model response time or tool execution.

## Session Files

### Reading Sessions

**Always use the dynamic-read-session tool** - Never read session JSON files directly as they can be large.

```typescript
// Use the dynamic-read-session tool
await dynamicReadSession({ sessionId: "uuid-here" })
```

Parameters:
- `sessionId`: The session UUID (found in logs or session filenames)
- `maxTurns`: Limit conversation turns (default: 50)

### Session File Locations

- Directory: `~/.acai/sessions/`
- Pattern: `session-{uuid}.json`

### Finding Session IDs

1. **From logs**: Look for `sessionId` field in error context
2. **From filenames**: Session files are named `session-{uuid}.json`
3. **From session list**: List files in `~/.acai/sessions/`

```bash
ls -la ~/.acai/sessions/ | tail -20
```

## Common Error Patterns

### Bad Request Errors

Check error log context for:
- `responseStatus`: HTTP status code (400 = Bad Request)
- `responseBody`: API error message
- `modelId`: Which model was being used
- `messageCount`: How many messages in context

### NoOutputGeneratedError

The model produced no output. Common causes:
- Invalid system prompt
- Model rate limiting
- Tool schema issues
- Context too long

### Session Restoration

When debugging session issues:
1. Find session ID from logs
2. Use dynamic-read-session to view the conversation
3. Check for truncation or missing messages

## Debugging Workflow

1. **Find the error in logs:**
   ```bash
   tail -100 ~/.acai/logs/current.log | grep -i error
   ```

2. **Extract session ID** from the error context

3. **Read the session** to understand what led to the error:
   ```bash
   # Use dynamic-read-session with the sessionId
   ```

4. **Check recent sessions:**
   ```bash
   ls -lt ~/.acai/sessions/ | head -10
   ```

## Manual Testing

When testing features manually, use tmux to run the REPL:

```bash
# Start a new tmux session
tmux new -s acai

# Run acai in dev mode
node source/index.ts

# Detach from tmux: Ctrl-b d

# Reattach to check logs
tail -f ~/.acai/logs/current.log
```
