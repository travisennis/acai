# ADR 004: Append-Only Session Task Events

**Status:** Accepted  
**Date:** 2026-05-03

## Context

cake sessions need to support long-lived conversations across multiple CLI invocations. Before format version 4, the persisted session shape was too close to live stream output and did not clearly distinguish resumable session files from redirected task streams. Older records such as `session_start`, `init`, and `result` also made loading ambiguous after session behavior changed.

We need a durable session log that:

1. Records session metadata exactly once.
2. Captures each CLI invocation as a distinct task.
3. Can be appended live as the agent emits conversation items.
4. Stores success, failure, duration, turn count, and usage for each task.
5. Keeps `--output-format stream-json` useful for integrations without making redirected output look resumable.

## Decision

Persisted session files use JSONL format version 4. The first record is `session_meta`. Each invocation appends:

1. `task_start`
2. Conversation records: `message`, `function_call`, `function_call_output`, and `reasoning`
3. `task_complete`

Session files are stored as flat `{session_id}.jsonl` files under `~/.local/share/cake/sessions/` or `$CAKE_DATA_DIR/sessions/`.

`--output-format stream-json` emits the same task and conversation record shapes for the current invocation, but never emits `session_meta` and never replays prior tasks. Redirected stream-json output is therefore an event stream, not a valid session file.

Path-based `--resume <path>` and `--fork <path>` are removed. Resume and fork accept UUIDs only.

## Rationale

- **Clear file identity**: A valid session file starts with `session_meta` and declares `format_version`.
- **Task boundaries**: `task_start` and `task_complete` make multi-invocation sessions inspectable without changing the conversation records restored into model context.
- **Live durability**: Conversation records are appended as they happen, so successful partial progress can survive a crash.
- **Integration clarity**: Stream-json remains a live feed for frontends and scripts, while persisted sessions remain the only resumable archive.
- **Simpler loading**: Rejecting legacy v2/v3 shapes avoids guessing whether a file is a session, a stream capture, or an unsupported older format.

## Consequences

- **Positive**: Session history can show task-level outcomes and aggregate token usage.
- **Positive**: Consumers can distinguish persisted sessions from live task streams by the presence of `session_meta`.
- **Positive**: Forking can seed only conversation records and omit parent task metadata.
- **Negative**: Existing v2 and old v3 session files no longer load.
- **Negative**: Users cannot resume from an arbitrary redirected JSONL file; they must use the persisted session UUID.
- **Negative**: Crash recovery can leave a trailing task without `task_complete`, so readers must tolerate incomplete final tasks.

## Alternatives Considered

- **Keep stream-json and session files identical**: Rejected because redirected output lacks session metadata and creates ambiguity around resume semantics.
- **Rewrite session files at task completion**: Rejected because it risks losing progress during long-running tasks and complicates concurrent access.
- **Store separate metadata and event files**: Rejected because a single append-only JSONL file is easier to inspect, copy, and lock.
- **Auto-migrate legacy v2/v3 sessions**: Rejected for now because the old shapes are ambiguous and migration can be handled later as an explicit tool if needed.

## References

- `docs/design-docs/session-management.md` - Persisted session lifecycle and schema
- `docs/design-docs/streaming-json-output.md` - Live task stream schema
- `src/clients/types.rs` - `SessionRecord`, `StreamRecord`, and task completion types
- `src/config/session.rs` - v4 session loading, appending, and compatibility checks
