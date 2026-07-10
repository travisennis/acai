---
status: accepted
date: 2026-07-02
decision-makers: Travis Ennis
---
# Piped Input Handling

## Context

The acai CLI tool needs to handle two distinct modes: interactive REPL mode and non-interactive CLI mode with piped input. These modes have different UX expectations and input handling requirements.

## Decision

### Mode Detection

The application detects input mode at startup:

```typescript
import { isatty } from "node:tty";

function isInteractive(): boolean {
  return process.stdin.isTTY === true;
}

function hasPipedInput(): boolean {
  return !process.stdin.isTTY;
}
```

### CLI Mode (Piped Input)

When input is piped (non-TTY), the application operates in single-shot mode:

```typescript
if (!process.stdin.isTTY) {
  // Read all input
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(chunk);
  }
  const input = Buffer.concat(chunks).toString("utf-8").trim();

  // Execute with input
  await executeWithPrompt({ prompt: input, config });

  // Exit after completion
  process.exit(0);
}
```

### REPL Mode (Interactive)

When stdin is a TTY, the application starts the interactive REPL:

```typescript
if (process.stdin.isTTY) {
  const repl = new REPL({
    terminal,
    config,
    workspace,
  });
  repl.start();
}
```

### Input Processing

Both modes use the same prompt processing pipeline:

```typescript
async function executeWithPrompt({
  prompt,
  config,
}: {
  prompt: string;
  config: Config;
}) {
  const sessionManager = new SessionManager({ ... });

  const userMessage = createUserMessage([], prompt);
  sessionManager.appendUserMessage(userMessage);

  const agent = new Agent({ ... });
  for await (const event of agent.run({ input: prompt, ... })) {
    // Handle events
  }
}
```

### Stdin Handling

The CLI module manages stdin configuration:

```typescript
export class StdinHandler {
  private buffer: string = "";
  private onLine: (line: string) => void;

  setup(onLine: (line: string) => void): void;
  feed(data: string): void;
  hasUnprocessedLines(): boolean;
  drainLine(): string | null;
  drainAll(): string;
}
```

### Signal Handling

Both modes handle signals consistently:

```typescript
// Graceful shutdown
process.on("SIGINT", () => {
  if (agent) {
    agent.abort();
  } else {
    process.exit(0);
  }
});

// Background (REPL only)
process.on("SIGTSTP", () => {
  terminal.background();
});
```

### TTY Control

The terminal module handles raw mode and control sequences:

```typescript
export class Terminal {
  start(
    onInput: (data: string) => void,
    onResize: () => void,
  ): void {
    // Set raw mode
    // Enable UTF-8
    // Enable mouse tracking
  }

  stop(): void {
    // Restore previous settings
  }

  background(): void {
    // Send to background (SIGTSTP equivalent)
  }
}
```

### Environment Variables

REPL-specific behavior is gated by TTY detection:

```typescript
// Only in REPL mode
if (process.stdin.isTTY) {
  // Enable editor mode
  // Enable bracketed paste
  // Enable mouse tracking
}
```

## Consequences

### Positive
- Clear mode distinction via TTY detection
- Shared execution pipeline ensures consistency
- CLI mode is scriptable and composable
- REPL mode provides rich interactive experience

### Negative
- Cannot mix modes (e.g., REPL with piped context)
- Stdin consumed after use, cannot be reused

### Examples

```bash
# CLI mode: single prompt
echo "Hello, write a hello world" | acai

# CLI mode: from file
acai < prompt.txt

# CLI mode: with arguments
acai -p "Explain this code" -f src/main.ts

# REPL mode: interactive
acai
```

## Alternatives Considered

**Hybrid Mode:** REPL that also accepts piped input as initial context. Adds complexity and unclear semantics. Not implemented.

**Daemon Mode:** Long-running server that accepts requests. Overkill for current use case. Not implemented.

