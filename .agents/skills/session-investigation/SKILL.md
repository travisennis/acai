---
name: session-investigation
description: Investigate cake CLI JSONL session files to identify actionable project improvements. Use when asked to read, review, audit, evaluate, or debug a cake session file or session UUID for tool call errors, repeated tool calls, tool result problems, permission issues, performance issues, missing context, prompt or instruction gaps, missing tools, session integrity problems, or poor agent behavior.
---

# Investigating Cake Sessions

## Overview

Use this skill to inspect persisted cake session files and produce a concise,
evidence-backed report on what cake could improve. Focus on concrete signals in
the session file and, when useful, corresponding cake logs.

## Inputs

Accept any of these inputs:

- Absolute session path, usually `~/.local/share/cake/sessions/{uuid}.jsonl`
- Session UUID, resolved as `~/.local/share/cake/sessions/{uuid}.jsonl`
- Custom data root from `CAKE_DATA_DIR`, resolved as
  `$CAKE_DATA_DIR/sessions/{uuid}.jsonl`

If the input is ambiguous, list recent candidates and choose by working
directory, timestamp, and visible task content.

## Session Format

Current persisted cake sessions use append-only JSONL format version 4:

1. First non-empty line: one `session_meta` record.
2. Each CLI invocation appends `task_start`.
3. The invocation may append `prompt_context` audit records.
4. Conversation records are `message`, `reasoning`, `function_call`, and
   `function_call_output`.
5. The invocation should end with `task_complete`.

Only conversation records are restored into model history. `prompt_context`
records are audit records for mutable context such as AGENTS.md, skills,
environment details, cwd, and date.

Treat files beginning with `session_start`, `init`, or `result` as legacy or
unsupported unless compatibility is the target of the investigation. Redirected
`stream-json` output is not a persisted session file because it lacks
`session_meta`.

## Workflow

### 1. Validate The File

- Confirm the file exists and is valid JSONL, allowing for a possible partial
  trailing record.
- Inspect the first record: `type`, `format_version`, `session_id`,
  `working_directory`, `model`, `tools`, `cake_version`, and git metadata.
- Identify whether the file is valid v4, legacy, unsupported, malformed, or a
  redirected stream-json feed.

### 2. Segment Tasks

- Group records from each `task_start` through its matching `task_complete`.
- Report task id, timestamp, duration, success, subtype, turns, usage, final
  result or error, and `permission_denials`.
- Flag trailing tasks without `task_complete`, incomplete assistant messages,
  malformed final records, abrupt endings, and suspicious status fields.

### 3. Reconstruct Conversation Flow

- Summarize each user request and final assistant response.
- Correlate every `function_call` with its `function_call_output` by `call_id`.
- Identify missing outputs, orphan outputs, duplicate ids, malformed arguments,
  invalid tool names, and unexpected ordering.

### 4. Inspect Tool Use

- Look for tool call errors, tool result errors, sandbox denials, permission
  denials, network failures, parse failures, missing files, command failures,
  failed tests, and edit conflicts.
- Detect repeated or near-identical tool calls, especially repeated reads,
  searches, failed commands, or edits that suggest the agent was stuck.
- Identify verbose tool results that likely polluted context: full logs, broad
  file dumps, unfiltered search results, complete build output, or repeated
  large outputs.
- Note when `rg`, `jq`, a targeted read, narrower command, structured parser,
  or better tool argument would have worked better.

### 5. Inspect Prompt And Context

- Review `prompt_context` for AGENTS.md, skills, environment context, cwd, date,
  sandbox permissions, and available tools.
- Identify missing, stale, unclear, or conflicting context that likely affected
  behavior.
- Check whether repository instructions were followed, especially verification
  after code, config, or dependency changes.
- Note missing tools, unclear tool descriptions, missing instructions, or
  permission guidance that would have helped.

### 6. Inspect Instruction Following

- Compare the user's request with the assistant's actions and final answer.
- Flag premature final answers, partial implementation, unnecessary questions,
  ignored constraints, missing verification, unreported blockers, overbroad
  changes, or poor preservation of user changes.
- Identify cases where the agent should have used a skill, read local docs
  first, asked for permission, or avoided a risky action.

### 7. Inspect Performance And Cost

- Use `task_complete.duration_ms`, `turn_count`, and `usage` to find slow,
  expensive, or inefficient tasks.
- Flag high reasoning tokens, many turns for simple tasks, repeated high-token
  tool outputs, large context growth, retry loops, likely truncation, and
  timeout risks.
- When useful, correlate session id and timestamps with
  `~/.cache/cake/cake.YYYY-MM-DD.log` or `$CAKE_DATA_DIR/cake.YYYY-MM-DD.log`.

## Issue Categories

Use these categories in findings:

- `tool_call_error`: tool invocation failed or returned an unexpected error.
- `tool_result_error`: tool output was malformed, incomplete, too verbose,
  misleading, or hard for the model to use.
- `repeated_tool_call`: same or equivalent call repeated without meaningful new
  information.
- `permission_issue`: sandbox, filesystem, approval, network, or OS permission
  handling blocked progress or was unclear.
- `performance_issue`: excessive duration, turn count, token use, output size,
  retries, or context growth.
- `missing_context`: needed project, environment, file, session, or prior-task
  context was absent.
- `prompt_or_instruction_gap`: cake prompts, AGENTS.md, skills, or tool docs did
  not provide enough guidance or created ambiguity.
- `instruction_following_issue`: agent ignored or only partly followed user,
  developer, project, or tool instructions.
- `missing_tool_or_capability`: an unavailable or poorly described tool,
  parser, permission path, or workflow would have materially helped.
- `session_integrity_issue`: malformed JSONL, missing task boundaries,
  unsupported format, missing records, orphan calls, duplicate ids, or
  incomplete final records.

## Evidence Requirements

For every finding, include:

- Record type and line number when available
- Task id and timestamp when available
- Tool name and `call_id` for tool-related findings
- Short excerpt of relevant user, assistant, or tool text
- Impact on the task
- Specific cake improvement recommendation

Do not paste huge tool outputs. Quote the smallest useful excerpt and summarize
the rest.

## Report Format

Return a concise report with:

1. `Executive Summary`: overall health and top three improvement opportunities.
2. `Session Metadata`: session id, format version, model, working directory,
   cake version, tools, task count, total duration, total turns, and token usage.
3. `Findings`: severity, category, evidence, impact, and recommendation.
4. `Task Timeline`: request, outcome, duration, turns, tools, errors, context.
5. `Tool Call Analysis`: counts by tool, failures, repeats, large outputs,
   permission denials, missing outputs, and orphan outputs.
6. `Prompt And Context Analysis`: whether context, AGENTS.md, skills, tools,
   and environment details were sufficient.
7. `Performance Notes`: slow tasks, high tokens, excessive turns, truncation or
   timeout risks.
8. `Recommended Cake Improvements`: prioritized implementation-ready changes.

## Useful Commands

```bash
head -1 "$SESSION" | jq '.'
jq -c '.' "$SESSION" >/dev/null
jq -r '.type' "$SESSION" | sort | uniq -c
jq 'select(.type == "task_complete")' "$SESSION"
jq 'select(.type == "prompt_context") | {task_id, role, timestamp, content: (.content[0:500])}' "$SESSION"
jq 'select(.type == "message" and .role == "user") | {timestamp, content}' "$SESSION"
jq 'select(.type == "message" and .role == "assistant") | {status, content: (.content[0:500])}' "$SESSION"
jq 'select(.type == "function_call") | {call_id, name, arguments}' "$SESSION"
jq 'select(.type == "function_call_output") | {call_id, output: (.output[0:1000])}' "$SESSION"
tail -5 "$SESSION" | jq '.'
```

For logs:

```bash
SESSION_ID="$(head -1 "$SESSION" | jq -r '.session_id')"
grep "$SESSION_ID" ~/.cache/cake/cake.*.log
```

Do not modify the session file during investigation.
