---
name: debugging-acai
description: |
  How to investigate and debug issues with the acai CLI tool. Use this skill whenever:
  - The user reports the CLI returned "None" or an empty response
  - The user mentions truncated, incomplete, or cut-off responses
  - The user says "Tool error" without explanation occurred
  - The user wants to debug why a task failed or behaved unexpectedly
  - The user asks about session files, logs, or how to investigate CLI behavior
  - The user needs to understand what happened during a previous CLI session
  - Any mention of debugging, investigating, or troubleshooting the acai CLI itself
---

# Debugging Acai CLI

This skill helps investigate and debug issues with the acai CLI tool.

## Quick Reference: Essential Commands

### Find Latest Session
```bash
# List all session directories
ls ~/.cache/acai/sessions/

# Find the directory hash for current project
echo -n "$(pwd)" | shasum -a 256 | cut -c1-16

# Quick way to find latest session for current directory
ls -la ~/.cache/acai/sessions/*/latest
```

### View Session Files

Sessions use JSON Lines (`.jsonl`) format. Use `jq -c` to process each line:

```bash
# View full session (all lines, pretty-printed)
jq '.' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl

# View session header (first line - metadata)
head -1 ~/.cache/acai/sessions/{hash}/{uuid}.jsonl | jq '.'

# View last 5 messages (most useful)
tail -5 ~/.cache/acai/sessions/{hash}/{uuid}.jsonl | jq '.'

# View all user prompts (see what was asked)
jq 'select(.type == "message" and .role == "user") | .content' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl

# View all assistant responses (see what was returned)
jq 'select(.role == "assistant") | .content' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl

# Check if response was complete (last line)
tail -1 ~/.cache/acai/sessions/{hash}/{uuid}.jsonl | jq '{type, status}'

# View all reasoning messages
jq 'select(.type == "reasoning")' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl

# View all tool calls
jq 'select(.type == "function_call")' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl

# View all tool outputs
jq 'select(.type == "function_call_output")' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl

# View tool calls AND outputs together (correlate calls with results)
jq 'select(.type == "function_call" or .type == "function_call_output")' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl

# Count messages by type (see conversation structure)
jq -r '.type' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl | sort | uniq -c

# Find what prompt caused a specific behavior (search by content)
jq 'select(.type == "message" and .role == "user") | select(.content | contains("refactor"))' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl
```

### Search Logs
```bash
# View recent log entries
tail -100 ~/.cache/acai/acai.log

# View logs in real-time
tail -f ~/.cache/acai/acai.log

# View recent errors (one-liner)
tail -50 ~/.cache/acai/acai.log | grep -i error

# Search for errors
grep -i "error" ~/.cache/acai/acai.log

# Search for warnings
grep -i "warn" ~/.cache/acai/acai.log

# Filter logs by date (when you know roughly when the issue occurred)
grep "2026-03-07" ~/.cache/acai/acai.log

# Find all API requests
grep "https://openrouter.ai" ~/.cache/acai/acai.log

# Find truncated outputs
grep "output truncated" ~/.cache/acai/acai.log
```

## Session Storage Structure

Sessions are stored in `~/.cache/acai/sessions/` organized by a hash of the working directory:

```
~/.cache/acai/sessions/
  {dir_hash}/           # First 16 hex chars of SHA-256 of working dir path
    {uuid}.jsonl        # Individual session files (JSON Lines format)
    latest -> {uuid}.jsonl  # Symlink to most recent session
```

### Finding Your Session Directory

```bash
# Find by looking at the latest symlink for each directory
for dir in ~/.cache/acai/sessions/*/; do
  echo "Directory: $(basename $dir)"
  ls -la "$dir/latest" 2>/dev/null
  echo "---"
done
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
tail -1 ~/.cache/acai/sessions/{hash}/{uuid}.jsonl | jq '{type, status}'
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
jq 'select(.type == "function_call_output") | {call_id, output: .output[0:200]}' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl
```

### 3. "Tool Error" Without Explanation

**Symptom**: CLI returns just "Tool error:" with no context.

**Investigation steps**:
1. Check the acai.log file around the time of the error
2. Look for the specific tool that failed
3. Check if it's a transient issue (network, file permissions, etc.)

### 4. Session Grew Too Large

**Check**:
```bash
# Check session file size
ls -lh ~/.cache/acai/sessions/{hash}/{uuid}.jsonl

# Count total lines (messages + header)
wc -l ~/.cache/acai/sessions/{hash}/{uuid}.jsonl

# Count total characters in all content fields
jq -r '.content // ""' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl | wc -c
```

### 5. Model Made Unexpected Tool Calls

**Check**:
```bash
# List all tool calls made
jq 'select(.type == "function_call") | {name, arguments}' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl
```

## Correlating Sessions with Logs

```bash
# 1. Get the session ID from the header line
SESSION_ID=$(head -1 ~/.cache/acai/sessions/{hash}/{uuid}.jsonl | jq -r '.session_id')
echo "Session ID: $SESSION_ID"

# 2. Find log entries around session creation time
TIMESTAMP=$(head -1 ~/.cache/acai/sessions/{hash}/{uuid}.jsonl | jq -r '.timestamp')
echo "Session start: $TIMESTAMP"

# 3. Search logs for that session's activity
grep "$SESSION_ID" ~/.cache/acai/acai.log
```

## Quick Reference Commands

```bash
# Find latest session for current directory
ls -la ~/.cache/acai/sessions/*/latest

# View last 5 messages (most common debugging command)
tail -5 ~/.cache/acai/sessions/*/latest | jq '.'

# Check if response was complete (last line)
tail -1 ~/.cache/acai/sessions/*/latest | jq '{type, status}'

# View recent errors in logs (one-liner)
tail -50 ~/.cache/acai/acai.log | grep -i error

# View full session file
less ~/.cache/acai/sessions/*/latest
```

The acai CLI has a built-in `ReadSession` tool that can read session files in a token-efficient format. This is useful when investigating issues programmatically.

```rust
// Read a session by ID (compact conversation history)
ReadSession {
    session_id: "04cddcba-3dd0-43f7-811c-829a5b0b9e87",
    max_turns: 50,  // Optional: limit conversation turns
}
```

## Debugging Checklist

When the user reports an issue:

1. **Find the session**
   - Locate the session directory using the hash of the working directory
   - Check the `latest` symlink

2. **Check for truncation**
   - `tail -1 session.jsonl | jq '{type, status}'` - should end with a completed message
   - If it ends with reasoning or has no status, the response was truncated

3. **Review the conversation flow**
   - `tail -5 session.jsonl | jq '.'` - see the last few interactions
   - Look for where things went wrong

4. **Check logs**
   - `tail -100 ~/.cache/acai/acai.log | grep -i error`
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
echo $ACAI_SANDBOX  # Should be unset or "on"

# Test a command in the sandbox with the same profile
sandbox-exec -f /tmp/acai/sandbox_profiles/acai_sandbox_*.sb bash -c "your-command-here"
```

### Using Trace Mode

Create a debug profile that logs denials instead of blocking them. Add a `(trace)` directive to the profile:

```bash
# 1. Find the generated profile
ls -la /tmp/acai/sandbox_profiles/

# 2. Copy it and add trace mode
cp /tmp/acai/sandbox_profiles/acai_sandbox_XXXX.sb /tmp/debug_sandbox.sb

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
# View the actual profile being used (check acai logs)
grep "Generated sandbox profile" ~/.cache/acai/acai.log

# Or find the latest profile file
ls -lt /tmp/acai/sandbox_profiles/ | head -5
cat /tmp/acai/sandbox_profiles/acai_sandbox_*.sb
```

## File Locations Summary

| File Type | Location |
|-----------|----------|
| Sessions | `~/.cache/acai/sessions/{hash}/{uuid}.jsonl` |
| Latest session symlink | `~/.cache/acai/sessions/{hash}/latest` |
| Logs | `~/.cache/acai/acai.log` |
| Config | `~/.cache/acai/` and `.acai` |
| User-level AGENTS.md | `~/.acai/AGENTS.md` |
| Project-level AGENTS.md | `./AGENTS.md` |

## Configuration

- **Config directory**: `~/.cache/acai/` and `.acai` (see `src/config/data_dir.rs`)
- **Logs**: `~/.cache/acai/acai.log`
- **API key**: Required via environment variable (default: `OPENCODE_ZEN_API_TOKEN`)

## Session Restoration and Continuation

To continue a previous session:

```bash
./target/release/acai instruct --continue --prompt "What was my last message?"
```

The `--continue` flag loads the latest session from the current directory.
