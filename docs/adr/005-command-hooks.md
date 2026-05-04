# ADR 005: Command Hooks

**Status:** Accepted  
**Date:** 2026-05-04

## Context

cake executes model-requested tools inside a controlled agent loop, but projects often need policy and workflow behavior that should not be hard-coded into the CLI. Examples include blocking unsafe shell commands, auditing tool usage, enriching a task with local context, and running cleanup or reporting logic when a task finishes.

These behaviors need access to lifecycle events and tool payloads, but they should remain local, inspectable, and project-specific. The implementation also needs to preserve cake's deterministic session history and avoid making every project automation a Rust feature.

## Decision

We add command hooks configured with JSON files loaded from global, project, and local locations:

1. `~/.config/cake/hooks.json`
2. `.cake/hooks.json`
3. `.cake/hooks.local.json`

Each hook file declares `version: 1`. Hooks are appended in load order, validate event names, and can use exact-match or `|`-separated matchers for supported events. Hook commands run through the platform shell with the project root as the working directory.

Hooks receive a JSON event payload on stdin and may return JSON on stdout. Supported lifecycle events are `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `Stop`, and `ErrorOccurred`.

`PreToolUse` hooks can allow, deny, or update a tool call before execution. Session and post-tool hooks can return additional context for the model. Hook failures are fail-open by default, with per-command `fail_closed` support when policy enforcement requires blocking behavior.

Hook activity is recorded through tracing and appended to session JSONL as `hook_event` records when sessions are enabled.

## Rationale

- **Project policy without CLI changes**: Teams can enforce local rules such as blocking dangerous Bash commands without adding project-specific logic to cake.
- **Small automation surface**: Shell commands make hooks easy to write in any language and easy to review in repository configuration.
- **Lifecycle coverage**: Session, prompt, tool, completion, and error events cover the main extension points users need without exposing the entire agent internals.
- **Deterministic configuration**: Global, project, and local hook files are loaded in a fixed append-only order, making behavior predictable.
- **Safe default failure mode**: Hooks fail open unless explicitly configured as `fail_closed`, avoiding accidental task breakage for auditing or context hooks.
- **Session observability**: Persisted hook records make policy decisions and automation effects inspectable alongside the conversation transcript.

## Consequences

- **Positive**: Users can customize policy, audit logging, and contextual automation without recompiling or forking cake.
- **Positive**: `PreToolUse` can block or rewrite tool calls before sandbox execution.
- **Positive**: Hook decisions and failures become visible in logs and session records.
- **Negative**: Hook commands are trusted local configuration and run outside the model tool sandbox.
- **Negative**: Hook execution adds latency to lifecycle events and tool calls.
- **Negative**: Hook authors must keep stdout valid JSON when returning decisions or context.

## Alternatives Considered

- **Hard-code common policies in cake**: Rejected because project policies vary widely and would expand the core CLI surface too quickly.
- **Add a Rust plugin API**: Rejected because it would be more complex to version, distribute, and sandbox than command hooks.
- **Use settings.toml for hook definitions**: Rejected because hook configuration is structured around event maps and command arrays, which fit JSON and the hook protocol examples more directly.
- **Fail closed by default**: Rejected because observability and context hooks should not break normal agent operation when a script is missing, slow, or invalid.
- **Run hooks inside the tool sandbox**: Rejected because hooks are local control-plane automation and often need access to project metadata, logs, or policy files that differ from model tool permissions.

## References

- `docs/design-docs/hooks.md` - Hook configuration, runtime protocol, and observability
- `.cake/hooks.json.example` - Example project hook configuration
- `src/config/hooks.rs` - Hook config loading, validation, and matcher handling
- `src/hooks.rs` - Hook runner, payloads, decisions, and session records
- `src/clients/agent.rs` - Tool-loop integration for pre-tool and post-tool hooks
- `src/main.rs` - Session lifecycle hook integration
