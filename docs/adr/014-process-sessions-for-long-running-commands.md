---
status: accepted
date: 2026-07-10
---
# Process Sessions for Long-Running Commands

## Context

The Bash tool executed every command through `child_process.exec` with a hard
timeout (default 90s). A command that outlived the timeout was killed with
SIGTERM. Partial output buffered up to that point was returned, but the work in
flight was destroyed and everything the command would have produced after the
deadline never existed. A five-minute build hitting the timeout had to be
restarted from scratch with a larger timeout.

The `background: true` escape hatch was worse: it spawned the process with
stdin ignored and routed stdout/stderr exclusively to `logger.debug`, returning
only a PID. There was no tool to retrieve output or poll status later, so
background output was invisible to the model unless it redirected to a file
itself. Interactive commands could not be driven at all.

## Decision

Replace the timeout-kill with a **yield window** backed by **process
sessions**, mirroring the exec_command/write_stdin model used by other agents:

- A new `ProcessSessionManager` (`source/execution/process-session.ts`) spawns
  every Bash command via `spawn` with all three stdio pipes and `detached:
  true` (making the child a process-group leader). If the command exits within
  the yield window, the result is identical to the old success path: output,
  exit code, duration. If it does not, the command is **not killed** — the call
  returns a short session id (`bash_<hex>`) plus output-so-far, and the process
  keeps running.
- The Bash tool's existing `timeout` parameter becomes the yield window
  (default unchanged at 90s), so there is no schema migration. `background:
  true` becomes a yield window of zero: it returns a session id immediately,
  and its output is retrievable instead of lost.
- A new `BashSession` tool interacts with yielded sessions: empty input polls
  for new output, non-empty input is written to stdin, and `kill: true`
  terminates the process group. Each read returns only output produced since
  the previous read (per-session cursor), and blocks up to a wait budget
  (default 10s) resolving early on new output or exit.

Lifecycle and safety semantics:

- Per-session unread output is bounded (~1MB); on overflow the oldest bytes are
  dropped and the next read notes the truncation. Tool results additionally
  pass through the Bash tool's existing 50KB context truncation.
- A user interrupt (abort signal) during the yield window kills the process
  group, matching the old interrupt behavior. Once a session has yielded, it is
  detached from the originating tool call's signal and outlives the call.
- Exited sessions are reaped once fully drained (after streams close), or after
  a 10-minute TTL. Running sessions are capped (16); all sessions are killed on
  acai shutdown via the same cleanup-hook pattern used for the old background
  processes.
- Pre-execution guardrails are unchanged: path validation, destructive-command
  blocking, multiline-commit detection, and the dangerous-command patterns
  (extracted as `validateCommandSafety`) all run before spawning.

## Rationale

- Fast commands behave exactly as before — one call, output plus exit code — so
  the model only deals with sessions when something genuinely runs long.
- Nothing is destroyed at an arbitrary deadline; long builds, test suites, dev
  servers, and interactive REPLs all become drivable.
- Killing the process group (negative pid) rather than the shell alone prevents
  orphaned grandchildren, which the old SIGTERM-to-shell approach leaked.

## Consequences

- `ExecutionEnvironment.executeCommand` and `executeCommandInBackground` are no
  longer used by the Bash tool; the class remains for other callers and tests.
- The model-facing contract changed: a `[still running | session: …]` footer
  replaces the old `Command timed out` message. Prompts and tool descriptions
  teach the poll flow.
- Sessions do not survive acai restarts; they are in-memory only.
- Stdin content written via `BashSession` does not pass through command
  validation (path checks, destructive-command blocking, dangerous-pattern
  matching) — those apply to the command string at spawn time. Stdin is
  arbitrary data by nature (passwords, heredocs, REPL input), so validating it
  as a command would produce false positives; the accepted trade-off is that
  an interactive shell started as a session can be driven past the
  command-string guards. This matches the write_stdin semantics of comparable
  agents.
