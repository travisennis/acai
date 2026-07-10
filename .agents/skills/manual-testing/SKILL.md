---
name: manual-testing
description: "Manually test acai features by running the app in tmux, interacting with the REPL, and inspecting logs and session files. Use after implementing a feature to verify it works end-to-end."
---

# Manual Testing

Run the acai REPL in a tmux session and interact with it to verify features work correctly. Inspect application logs and session files to confirm internal behavior.

## Quick Reference

- **Start session:** `tmux new-session -d -s acai-test "node source/index.ts"`
- **Send text:** `tmux send-keys -t acai-test -l "your prompt"`
- **Submit (Shift+Enter):** `tmux send-keys -t acai-test Escape "[13;2u"`
- **Send Shift+Tab:** `tmux send-keys -t acai-test Escape "[9;2u"`
- **Send Ctrl+N:** `tmux send-keys -t acai-test C-n`
- **Send Ctrl+O:** `tmux send-keys -t acai-test C-o`
- **Send Escape:** `tmux send-keys -t acai-test Escape`
- **Read screen:** `tmux capture-pane -t acai-test -p -S -100`
- **Kill session:** `tmux kill-session -t acai-test`

## Workflow

### 1. Start the App

```bash
tmux kill-session -t acai-test 2>/dev/null
tmux new-session -d -s acai-test "node source/index.ts"
sleep 2
```

Wait for the welcome screen to appear before interacting:

```bash
tmux capture-pane -t acai-test -p -S -50
```

Confirm you see the welcome message and the editor prompt.

### 2. Interact with the REPL

Type text into the editor:

```bash
tmux send-keys -t acai-test -l "hello, what mode are you in?"
```

Submit with Shift+Enter (kitty keyboard protocol):

```bash
tmux send-keys -t acai-test Escape "[13;2u"
```

Wait for the agent to respond, then read the output:

```bash
sleep 5
tmux capture-pane -t acai-test -p -S -100
```

For longer agent runs, poll until the agent finishes (the editor prompt reappears):

```bash
for i in $(seq 1 30); do
  output=$(tmux capture-pane -t acai-test -p -S -5)
  if echo "$output" | grep -q "›"; then
    break
  fi
  sleep 2
done
tmux capture-pane -t acai-test -p -S -100
```

### 3. Send Keyboard Shortcuts

Use tmux `send-keys` for special keys:

| Action | Command |
|--------|---------|
| Shift+Tab | `tmux send-keys -t acai-test Escape "[9;2u"` |
| Ctrl+N (new chat) | `tmux send-keys -t acai-test C-n` |
| Ctrl+O (verbose) | `tmux send-keys -t acai-test C-o` |
| Escape (interrupt) | `tmux send-keys -t acai-test Escape` |
| Enter | `tmux send-keys -t acai-test Enter` |
| Tab | `tmux send-keys -t acai-test Tab` |

### 4. Inspect Application Logs

Logs are written to `~/.acai/logs/current.log` in JSON format (one JSON object per line):

```bash
tail -20 ~/.acai/logs/current.log
```

Filter for specific log levels or messages:

```bash
grep '"level":"ERROR"' ~/.acai/logs/current.log | tail -10
grep '"level":"WARN"' ~/.acai/logs/current.log | tail -10
```

Search for specific events:

```bash
grep 'saved' ~/.acai/logs/current.log | tail -5
```

### 5. Inspect Session Files

Session files are stored at `~/.acai/sessions/` as JSON:

```bash
ls -lt ~/.acai/sessions/ | head -5
```

Read the most recent session:

```bash
ls -t ~/.acai/sessions/ | head -1 | xargs -I{} cat ~/.acai/sessions/{}
```

Check specific fields (e.g., metadata for mode state):

```bash
ls -t ~/.acai/sessions/ | head -1 | xargs -I{} node -e "
  const s = require('fs').readFileSync(process.env.HOME + '/.acai/sessions/{}', 'utf8');
  const j = JSON.parse(s);
  console.log('title:', j.title);
  console.log('modelId:', j.modelId);
  console.log('messages:', j.messages.length);
  console.log('metadata:', JSON.stringify(j.metadata, null, 2));
"
```

### 6. Cleanup

Always kill the tmux session when done:

```bash
tmux kill-session -t acai-test
```

## Verification Patterns

### Verify Footer Content

Capture the bottom of the screen where the footer renders:

```bash
tmux capture-pane -t acai-test -p -S -10
```

Look for mode indicators, model info, git status, and token usage in the output.

### Verify Mode Cycling

```bash
# Press Shift+Tab and check footer
tmux send-keys -t acai-test Escape "[9;2u"
sleep 0.5
tmux capture-pane -t acai-test -p -S -10
```

### Verify Session Persistence

1. Submit a prompt so the session gets saved
2. Read the session file and check for expected fields
3. Kill the session, restart, resume via `/history`, and verify state restored

### Verify a Prompt Reaches the LLM

1. Submit a prompt
2. Wait for the agent to finish
3. Read the session file and inspect the `messages` array to confirm user messages, mode context messages, and assistant responses are present

## Tips

- Always `sleep` after sending keys — the app needs time to process input and render
- Use `-S -N` with `capture-pane` to control how many lines of scrollback to read (e.g., `-S -200` for 200 lines)
- If the session seems stuck, check `~/.acai/logs/current.log` for errors
- The REPL uses the kitty keyboard protocol for Shift+Enter (`Escape "[13;2u"`) and Shift+Tab (`Escape "[9;2u"`)
- Session files may be large; use `node -e` or `jq` to extract specific fields rather than reading the whole file
