---
name: debugging-cake
description: |
  How to investigate and debug issues with the cake CLI tool. Use this skill whenever:
  - The user reports the CLI returned "None" or an empty response
  - The user mentions truncated, incomplete, or cut-off responses
  - The user says "Tool error" without explanation occurred
  - The user wants to debug why a task failed or behaved unexpectedly
  - The user asks about session files, logs, or how to investigate CLI behavior
  - The user needs to understand what happened during a previous CLI session
  - Any mention of debugging, investigating, or troubleshooting the cake CLI itself
---

# Debugging Cake CLI

This skill helps investigate and debug issues with the cake CLI tool.

## Quick Reference: Essential Commands

### Find Latest Session
```bash
# List all session files
ls ~/.local/share/cake/sessions/

# Find the latest session for current directory
ls -t ~/.local/share/cake/sessions/*.jsonl 2>/dev/null | head -1

# Quick way to find latest session for current directory (most recently modified .jsonl)
ls -t ~/.local/share/cake/sessions/*.jsonl 2>/dev/null | head -1
```

### View Session Files

Sessions use JSON Lines (`.jsonl`) format. Use `jq -c` to process each line:

```bash
# View full session (all lines, pretty-printed)
jq '.' ~/.local/share/cake/sessions/{uuid}.jsonl

# View session header (first line - metadata)
head -1 ~/.local/share/cake/sessions/{uuid}.jsonl | jq '.'

# View last 5 messages (most useful)
tail -5 ~/.local/share/cake/sessions/{uuid}.jsonl | jq '.'

# View all user prompts (see what was asked)
jq 'select(.type == "message" and .role == "user") | .content' ~/.local/share/cake/sessions/{uuid}.jsonl

# View all assistant responses (see what was returned)
jq 'select(.role == "assistant") | .content' ~/.local/share/cake/sessions/{uuid}.jsonl

# Check if response was complete (last line)
tail -1 ~/.local/share/cake/sessions/{uuid}.jsonl | jq '{type, status}'

# View all reasoning messages
jq 'select(.type == "reasoning")' ~/.local/share/cake/sessions/{uuid}.jsonl

# View all tool calls
jq 'select(.type == "function_call")' ~/.local/share/cake/sessions/{uuid}.jsonl

# View all tool outputs
jq 'select(.type == "function_call_output")' ~/.local/share/cake/sessions/{uuid}.jsonl

# View tool calls AND outputs together (correlate calls with results)
jq 'select(.type == "function_call" or .type == "function_call_output")' ~/.local/share/cake/sessions/{uuid}.jsonl

# Count messages by type (see conversation structure)
jq -r '.type' ~/.local/share/cake/sessions/{uuid}.jsonl | sort | uniq -c

# Find what prompt caused a specific behavior (search by content)
jq 'select(.type == "message" and .role == "user") | select(.content | contains("refactor"))' ~/.local/share/cake/sessions/{uuid}.jsonl
```

### Search Logs

Log files use daily rotation with naming `cake.YYYY-MM-DD.log`. The dated file IS the current
log file for that day - there is no separate "current" file without a date.

```bash
# View today's log entries
tail -100 ~/.cache/cake/cake.$(date +%Y-%m-%d).log

# View logs in real-time
tail -f ~/.cache/cake/cake.$(date +%Y-%m-%d).log

# View recent errors (one-liner)
tail -50 ~/.cache/cake/cake.$(date +%Y-%m-%d).log | grep -i error

# Search for errors in today's log
grep -i "error" ~/.cache/cake/cake.$(date +%Y-%m-%d).log

# Search for warnings
grep -i "warn" ~/.cache/cake/cake.$(date +%Y-%m-%d).log

# Search across all log files
grep -i "error" ~/.cache/cake/cake.*.log

# Find all API requests
grep "https://opencode.ai" ~/.cache/cake/cake.*.log

# Find truncated outputs
grep "output truncated" ~/.cache/cake/cake.*.log

# List all log files
ls -la ~/.cache/cake/cake.*.log
```

## Session Storage Structure

Sessions are stored in `~/.local/share/cake/sessions/` (or `$CAKE_DATA_DIR/sessions/`) as flat `.jsonl` files:

```
~/.local/share/cake/sessions/
  {uuid}.jsonl          # Individual session files (JSON Lines format)
```

The most recent session is determined by file modification time (no symlink needed).

### Finding Your Session Directory

```bash
# Find the most recently modified session
ls -lt ~/.local/share/cake/sessions/*.jsonl 2>/dev/null | head -5
```

### Session File Structure

Sessions use JSON Lines (`.jsonl`) format. Each line is a valid JSON object:

**Line 1: Session Header**
```json
{
  "format_version": 2,
  "session_id": "uuid-v4",
  "timestamp": "2026-03-28T12:00:00Z",
  "working_directory": "/absolute/path/to/project",
  "model": "model-name",
  "type": "session_start"
}
```

**Subsequent Lines: Conversation Items**
```json
{
  "format_version": 2,
  "session_id": "uuid-v4",
  "timestamp": "2026-03-28T12:00:01Z",
  "working_directory": "/absolute/path/to/project",
  "type": "message",
  "role": "user",
  "content": "Hello"
}
```

Each conversation item (message, function_call, function_call_output, reasoning) is on its own line.

### Message Types

- `message` - User or assistant text messages
- `reasoning` - Model's internal reasoning (if supported by model)
- `function_call` - Tool invocation request
- `function_call_output` - Result of tool execution

## Common Debugging Patterns

### 1. Response Was Truncated (Root Cause of "None" Output)

**Symptom**: CLI returns `None` instead of a meaningful response.

**Check**:
```bash
# A complete response ends with type: "message" and status: "completed"
# If it ends with "reasoning" or has no status, it was truncated
tail -1 ~/.local/share/cake/sessions/{uuid}.jsonl | jq '{type, status}'
```

**Example truncated response** (last line of `.jsonl` file):
```json
{"format_version":2,"session_id":"abc-123","timestamp":"2026-03-28T12:00:00Z","working_directory":"/work","type":"reasoning","id":"rs_tmp_tf8nkow8vrp","summary":["Now"]}
```
Note: The `summary` array is cut off mid-sentence (incomplete reasoning).

**How to investigate**:
1. Find the session directory
2. View the last few messages to see where it ended
3. Check logs for "output truncated" messages
4. Look at the reasoning messages to understand what the model was doing

### 2. Tool Execution Failed

**Check**:
```bash
# Find all function_call_output messages and check for errors
jq 'select(.type == "function_call_output") | {call_id, output: .output[0:200]}' ~/.local/share/cake/sessions/{uuid}.jsonl
```

### 3. "Tool Error" Without Explanation

**Symptom**: CLI returns just "Tool error:" with no context.

**Investigation steps**:
1. Check the log file for that day: `~/.cache/cake/cake.YYYY-MM-DD.log`
2. Look for the specific tool that failed
3. Check if it's a transient issue (network, file permissions, etc.)

### 4. Session Grew Too Large

**Check**:
```bash
# Check session file size
ls -lh ~/.local/share/cake/sessions/{uuid}.jsonl

# Count total lines (messages + header)
wc -l ~/.local/share/cake/sessions/{uuid}.jsonl

# Count total characters in all content fields
jq -r '.content // ""' ~/.local/share/cake/sessions/{uuid}.jsonl | wc -c
```

### 5. Model Made Unexpected Tool Calls

**Check**:
```bash
# List all tool calls made
jq 'select(.type == "function_call") | {name, arguments}' ~/.local/share/cake/sessions/{uuid}.jsonl
```

## Correlating Sessions with Logs

```bash
# 1. Get the session ID from the header line
SESSION_ID=$(head -1 ~/.local/share/cake/sessions/{uuid}.jsonl | jq -r '.session_id')
echo "Session ID: $SESSION_ID"

# 2. Find log entries around session creation time
TIMESTAMP=$(head -1 ~/.local/share/cake/sessions/{uuid}.jsonl | jq -r '.timestamp')
echo "Session start: $TIMESTAMP"

# 3. Search logs for that session's activity
grep "$SESSION_ID" ~/.cache/cake/cake.*.log
```

## Quick Reference Commands

```bash
# Find latest session for current directory
LATEST=$(ls -t ~/.local/share/cake/sessions/*.jsonl 2>/dev/null | head -1)

# View last 5 messages (most common debugging command)
tail -5 "$LATEST" | jq '.'

# Check if response was complete (last line)
tail -1 "$LATEST" | jq '{type, status}'

# View recent errors in logs (one-liner)
tail -50 ~/.cache/cake/cake.$(date +%Y-%m-%d).log | grep -i error

# View full session file
less "$LATEST"
```

## Debugging Checklist

When the user reports an issue:

1. **Find the session**
   - List session files in `~/.local/share/cake/sessions/`
   - Find the most recently modified `.jsonl` file

2. **Check for truncation**
   - `tail -1 session.jsonl | jq '{type, status}'` - should end with a completed message
   - If it ends with reasoning or has no status, the response was truncated

3. **Review the conversation flow**
   - `tail -5 session.jsonl | jq '.'` - see the last few interactions
   - Look for where things went wrong

4. **Check logs**
   - `tail -100 ~/.cache/cake/cake.$(date +%Y-%m-%d).log | grep -i error`
   - Look for tool failures or API errors

5. **Identify patterns**
   - Were there multiple rapid tool calls?
   - Did the model get stuck in a loop?
   - Was there a specific error message?

## Key Insight: Why "None" Happens

The most common cause of `None` output is **response truncation**. When the model's response is cut off mid-generation (often during reasoning), the CLI has no complete message to return, so it returns `None`.

This typically happens when:
- The model hits token limits
- The response times out
- The streaming connection is interrupted

**Fix approach**: The CLI should detect incomplete responses and either:
- Automatically retry/continue
- Warn the user that the task may be incomplete
- Return a meaningful message instead of `None`

## Debugging Sandbox Violations

When commands fail with `Operation not permitted (os error 1)` inside the sandbox, use `sandbox-exec`'s trace mode to identify exactly which operations are being denied.

### Quick Diagnosis

```bash
# Check if sandbox is active
echo $CAKE_SANDBOX  # Should be unset or "on"

# Test a command in the sandbox with the same profile
sandbox-exec -f /tmp/cake/sandbox_profiles/cake_sandbox_*.sb bash -c "your-command-here"
```

### Using Trace Mode

Create a debug profile that logs denials instead of blocking them. Add a `(trace)` directive to the profile:

```bash
# 1. Find the generated profile
ls -la /tmp/cake/sandbox_profiles/

# 2. Copy it and add trace mode
cp /tmp/cake/sandbox_profiles/cake_sandbox_XXXX.sb /tmp/debug_sandbox.sb

# 3. Edit to add trace output — replace "(deny default)" with:
#    (deny default (with send-signal SIGKILL))
#    (trace "/tmp/sandbox_trace.log")
# Or for just logging without blocking:
#    (deny default (with no-log))
#    (trace "/tmp/sandbox_trace.log")

# 4. Run the failing command with the debug profile
sandbox-exec -f /tmp/debug_sandbox.sb bash -c "cargo check"

# 5. View the trace to see what operations were denied
cat /tmp/sandbox_trace.log
```

### Common Missing Permissions

| Error Pattern | Likely Cause | Fix |
|---|---|---|
| `Operation not permitted` on `target/` writes | Missing `file-lock` | Add `(allow file-lock)` to profile |
| `/tmp` access denied despite being allowed | Symlink mismatch (`/tmp` → `/private/tmp`) | Ensure both forms in profile |
| Cargo registry download fails | `~/.cargo/registry` is read-only | Add to `read_write` paths |
| `flock` / `fcntl` failures | Missing `file-lock` permission | Add `(allow file-lock)` to profile |

### Inspecting the Generated Profile

```bash
# View the actual profile being used (check cake logs)
grep "Generated sandbox profile" ~/.cache/cake/cake.*.log

# Or find the latest profile file
ls -lt /tmp/cake/sandbox_profiles/ | head -5
cat /tmp/cake/sandbox_profiles/cake_sandbox_*.sb
```

## File Locations Summary

| File Type | Location |
|-----------|----------|
| Sessions | `~/.local/share/cake/sessions/{uuid}.jsonl` |
| Logs | `~/.cache/cake/cake.YYYY-MM-DD.log` |
| Global config | `~/.config/cake/settings.toml` |
| Project config | `.cake/settings.toml` |
| User-level AGENTS.md | `~/.cake/AGENTS.md` |
| Project-level AGENTS.md | `./AGENTS.md` |

## Configuration

- **Cache directory**: `~/.cache/cake/` (logs and ephemeral data)
- **Sessions directory**: `~/.local/share/cake/sessions/` (session files)
- **Data directory override**: Set `CAKE_DATA_DIR` to use a custom path for both cache and sessions
- **Logs**: `~/.cache/cake/cake.YYYY-MM-DD.log` (or `$CAKE_DATA_DIR/cake.YYYY-MM-DD.log` if set, daily rotation)
- **API key**: Required via environment variable (default: `OPENCODE_ZEN_API_TOKEN`)

## Session Restoration and Continuation

To continue a previous session:

```bash
./target/release/cake --continue "What was my last message?"
```

The `--continue` flag loads the latest session from the current directory.
