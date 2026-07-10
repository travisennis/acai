---
name: verify
description: How to build and drive acai end-to-end to verify changes at its real surface.
---

# Verifying acai changes

## Build and launch

```bash
npm run build          # compiles to dist/; bin/acai runs dist/index.js
./bin/acai -p "..."    # one-shot CLI mode (no alternate screen, easy to capture)
```

CLI mode (`-p`) drives the full real agent loop — model, tool registry,
prompts — and prints the final answer plus a session summary to stdout.
It uses the user's real model config (`~/.acai/acai.json`) and API keys
from the environment, so each run makes billed model calls; keep prompts
small and end them with "Do not run any other commands."

## Driving tool behavior

To verify tool-level behavior, tell the model exactly which tool to call
and with which parameters, then ask it to report the tool result verbatim
(e.g. "report the exact metadata footer line in square brackets"). This
reliably exercises Bash/BashSession/etc. through the real agent.

## Gotchas

- `timeout(1)` does not exist on this macOS host; bound long runs with the
  Bash tool's own timeout instead.
- Process sessions (Bash yield/BashSession) are in-memory per acai process;
  they cannot be verified across separate `-p` invocations. Verify the whole
  flow inside a single prompt.
- Orphan checks: after acai exits, `pgrep -lf <command>` confirms session
  cleanup killed the process group.
- stdout includes terminal title escape sequences (`]0;...`); pipe through
  `tail` or strip them when capturing.
