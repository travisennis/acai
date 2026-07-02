---
status: accepted
date: 2026-05-06
decision-makers: Travis Ennis
---
# Remove Specialized File Search and Agent Tools

## Context

acai currently ships 14 built-in tools. Five of these, Glob, Grep, Ls, DirectoryTree, and Agent, provide specialized functionality that overlaps with capabilities available through the Bash tool. Modern language models are proficient at composing shell commands (`rg`, `find`, `ls`, `tree`) to achieve the same results without dedicated tool abstractions.

Maintaining these tools carries real costs: each one requires schema definitions, input validation, output formatting, error handling, and ongoing test coverage. The Glob and Grep tools alone accounted for 7 test files and a `fast-glob` dependency.

## Decision

Remove the following five tools from acai:

| Tool | Replacement |
|------|-------------|
| `Glob` | `find` via Bash |
| `Grep` | `rg` (ripgrep) via Bash |
| `Ls` | `ls` via Bash |
| `DirectoryTree` | `tree` or `find` via Bash |
| `Agent` | Removed entirely (subagent system removed) |

Additionally, remove the supporting infrastructure that only these tools consumed:

- `source/agent/sub-agent.ts` (SubAgent class)
- `source/subagents/index.ts` (subagent discovery and prompt formatting)
- `source/utils/glob.ts` (fast-glob wrapper)
- `fast-glob` npm dependency

The system prompt is updated to instruct models to use `rg` and `find` via Bash for code exploration instead of dedicated Grep/Glob tools.

## Consequences

### Positive

- Smaller attack surface for tool schemas and fewer edge cases to maintain
- Fewer dependencies (fast-glob removed)
- Reduced token overhead per request (5 fewer tool definitions sent to the model)
- Simpler mental model: Bash handles all file system search and listing operations

### Negative

- Models must compose shell commands for search/list operations rather than using structured tool inputs
- Shell command output is less structured than dedicated tool output, which may require more parsing by the model
- Subagent delegation is no longer available as a built-in capability

### Alternatives Considered

**Keep tools but mark as optional:** Would still require maintenance and add token overhead. Not worth the cost for tools that Bash can replace.

**Replace with lighter wrappers:** Would still require schema maintenance and dependency management. The Bash tool already provides full shell access.

## More Information

- Supersedes [ADR-005](005-sub-agent-communication.md).
