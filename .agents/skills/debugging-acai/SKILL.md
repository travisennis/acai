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
```bash
# View full session (pretty-printed)
jq '.' ~/.cache/acai/sessions/{hash}/{uuid}.json

# View session metadata (quick overview)
jq '{id, working_dir, created_at, updated_at}' ~/.cache/acai/sessions/{hash}/{uuid}.json

# View last 5 messages (most useful)
jq '.messages[-5:]' ~/.cache/acai/sessions/{hash}/{uuid}.json

# View all user prompts (see what was asked)
jq '.messages[] | select(.type == "message" and .role == "user") | .content' ~/.cache/acai/sessions/{hash}/{uuid}.json

# View all assistant responses (see what was returned)
jq '.messages[] | select(.role == "assistant") | .content' ~/.cache/acai/sessions/{hash}/{uuid}.json

# Check if response was complete
jq '.messages[-1] | {type, status}' ~/.cache/acai/sessions/{hash}/{uuid}.json

# View all reasoning messages
jq '.messages[] | select(.type == "reasoning")' ~/.cache/acai/sessions/{hash}/{uuid}.json

# View all tool calls
jq '.messages[] | select(.type == "function_call")' ~/.cache/acai/sessions/{hash}/{uuid}.json

# View all tool outputs
jq '.messages[] | select(.type == "function_call_output")' ~/.cache/acai/sessions/{hash}/{uuid}.json

# View tool calls AND outputs together (correlate calls with results)
jq '.messages[] | select(.type == "function_call" or .type == "function_call_output")' ~/.cache/acai/sessions/{hash}/{uuid}.json

# Count messages by type (see conversation structure)
jq '[.messages[].type] | group_by(.) | map({type: .[0], count: length})' ~/.cache/acai/sessions/{hash}/{uuid}.json

# Find what prompt caused a specific behavior (search by content)
jq '.messages[] | select(.type == "message" and .role == "user") | select(.content | contains("refactor"))' ~/.cache/acai/sessions/{hash}/{uuid}.json
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
    {uuid}.json         # Individual session files
    latest -> {uuid}.json  # Symlink to most recent session
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

```json
{
  "format_version": 1,
  "id": "uuid-v4",
  "working_dir": "/absolute/path/to/project",
  "created_at": 1772918641731,    // Unix timestamp in milliseconds
  "updated_at": 1772918724253,
  "messages": [...]
}
```

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
jq '.messages[-1] | {type, status}' ~/.cache/acai/sessions/{hash}/{uuid}.json
```

**Example truncated response**:
```json
{
  "type": "reasoning",
  "id": "rs_tmp_tf8nkow8vrp",
  "summary": ["Now"]  // Cut off mid-sentence!
}
```

**How to investigate**:
1. Find the session directory
2. View the last few messages to see where it ended
3. Check logs for "output truncated" messages
4. Look at the reasoning messages to understand what the model was doing

### 2. Tool Execution Failed

**Check**:
```bash
# Find all function_call_output messages and check for errors
jq '.messages[] | select(.type == "function_call_output") | {call_id, output: .output[0:200]}' ~/.cache/acai/sessions/{hash}/{uuid}.json
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
ls -lh ~/.cache/acai/sessions/{hash}/{uuid}.json

# Count total messages
jq '.messages | length' ~/.cache/acai/sessions/{hash}/{uuid}.json

# Count total characters in all messages
jq '[.messages[].content // ""] | add | length' ~/.cache/acai/sessions/{hash}/{uuid}.json
```

### 5. Model Made Unexpected Tool Calls

**Check**:
```bash
# List all tool calls made
jq '.messages[] | select(.type == "function_call") | {name, arguments}' ~/.cache/acai/sessions/{hash}/{uuid}.json
```

## Correlating Sessions with Logs

```bash
# 1. Get the session ID
SESSION_ID=$(jq -r '.id' ~/.cache/acai/sessions/{hash}/{uuid}.json)
echo "Session ID: $SESSION_ID"

# 2. Find log entries around session creation time
CREATED_AT=$(jq -r '.created_at' ~/.cache/acai/sessions/{hash}/{uuid}.json)
date -r $((CREATED_AT / 1000))  # Convert to human-readable

# 3. Search logs for that session's activity
grep "$SESSION_ID" ~/.cache/acai/acai.log
```

## Quick Reference Commands

```bash
# Find latest session for current directory
ls -la ~/.cache/acai/sessions/*/latest

# View last 5 messages (most common debugging command)
jq '.messages[-5:]' ~/.cache/acai/sessions/*/latest

# Check if response was complete
jq '.messages[-1] | {type, status}' ~/.cache/acai/sessions/*/latest

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
   - `jq '.messages[-1]'` - should end with a completed message
   - If it ends with reasoning or has no status, the response was truncated

3. **Review the conversation flow**
   - `jq '.messages[-5:]'` - see the last few interactions
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

## File Locations Summary

| File Type | Location |
|-----------|----------|
| Sessions | `~/.cache/acai/sessions/{hash}/{uuid}.json` |
| Latest session symlink | `~/.cache/acai/sessions/{hash}/latest` |
| Logs | `~/.cache/acai/acai.log` |
| Config | `~/.cache/acai/` |

## Session Restoration and Continuation

To continue a previous session:

```bash
./target/release/acai instruct --continue --prompt "What was my last message?"
```

The `--continue` flag loads the latest session from the current directory.
