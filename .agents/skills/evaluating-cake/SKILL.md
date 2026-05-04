---
name: evaluating-cake
description: Evaluate cake CLI session performance to identify issues and recommend improvements. Use this skill when asked to evaluate, assess, review, or analyze how well cake performed on a task, or when investigating session quality for improvements to the project.
---

# Evaluating Cake CLI Sessions

This skill guides systematic evaluation of cake CLI sessions to identify issues and recommend improvements.

## When to Use This Skill

- After completing a task with cake to evaluate performance
- When investigating why a task didn't go well
- When looking for patterns across sessions
- When documenting issues for project improvement

## Overview

The evaluation process analyzes a session to identify:

1. **Task completion quality** - Did the agent accomplish the goal?
2. **Reasoning patterns** - How did the agent approach the problem?
3. **Tool usage** - Were tools used effectively and appropriately?
4. **Knowledge gaps** - What context was missing or misunderstood?
5. **System prompt issues** - Were there behavioral or reasoning flaws?

## Phase 0: Understand CLI Capabilities

Before evaluating, understand what tools the cake CLI has available. This ensures evaluation is based on actual capabilities, not assumptions.

1. Read these files:
   - `ARCHITECTURE.md` - Overall architecture
   - `src/clients/tools/mod.rs` - Available tools
   - `src/clients/tools/*.rs` - Tool capabilities and parameters

2. Key distinction: **You have different tools than the cake CLI**. Only identify issues valid for the CLI's actual toolset.

## Phase 1: Locate and Read the Session

### Finding Sessions

Sessions are stored in `~/.local/share/cake/sessions/` as flat `.jsonl` files, or in `$CAKE_DATA_DIR/sessions/` when that override is set:

```bash
# List all session files
ls ~/.local/share/cake/sessions/

# Find latest session (most recently modified .jsonl)
ls -t ~/.local/share/cake/sessions/*.jsonl 2>/dev/null | head -1
```

### Reading Sessions

Read the raw JSONL file. Current persisted sessions are append-only format version 4: the first record is `session_meta`, each invocation starts with `task_start`, may include `prompt_context` audit records, and should end with `task_complete`.

```bash
# View session metadata
head -1 ~/.local/share/cake/sessions/{uuid}.jsonl | jq '.'

# View full session
jq '.' ~/.local/share/cake/sessions/{uuid}.jsonl

# View last 10 records (often most relevant for evaluation)
tail -10 ~/.local/share/cake/sessions/{uuid}.jsonl | jq '.'

# View all user prompts
jq 'select(.type == "message" and .role == "user") | .content' session.jsonl

# View all assistant responses
jq 'select(.role == "assistant") | .content' session.jsonl

# View all reasoning
jq 'select(.type == "reasoning")' session.jsonl

# View all tool calls
jq 'select(.type == "function_call") | {name, arguments}' session.jsonl

# View tool calls with outputs
jq 'select(.type == "function_call" or .type == "function_call_output")' session.jsonl

# View prompt/context audit records
jq 'select(.type == "prompt_context") | {task_id, role, timestamp, content: (.content[0:500])}' session.jsonl

# View task outcomes
jq 'select(.type == "task_complete") | {task_id, subtype, success, duration_ms, turn_count, result, error, usage, permission_denials}' session.jsonl

# Count message types
jq -r '.type' session.jsonl | sort | uniq -c
```

Treat files whose first record is `session_start`, `init`, or `result` as legacy or unsupported unless the evaluation is specifically about compatibility. Redirected `--output-format stream-json` output is not a resumable persisted session file because it does not start with `session_meta`.

## Phase 2: Evaluate Task Completion

### Did the Agent Complete the Task?

1. Identify the original user request from the first user message
2. Trace through the conversation to see what was accomplished
3. Check the final assistant response for completion indicators

**Completion indicators:**
- Explicit confirmation of task completion
- Summary of changes made
- Clear answer to the question asked

**Non-completion indicators:**
- Response ends mid-sentence (truncation)
- Agent asks clarifying questions without resolution
- Task was partially completed
- Agent gave up or hit an error

### Quality Assessment

Evaluate the quality of the work:

- **Correctness**: Was the solution correct?
- **Completeness**: Were all requirements addressed?
- **Efficiency**: Was the approach efficient or overly complicated?
- **Code quality**: If code was written, was it well-structured?

## Phase 3: Analyze Agent Behavior

### Reasoning Patterns

Review reasoning messages to understand the agent's thought process:

```bash
# View all reasoning
jq 'select(.type == "reasoning")' session.jsonl
```

Look for:
- **Clear problem decomposition** - Did the agent break down the task well?
- **Appropriate planning** - Was there a logical sequence?
- **Self-correction** - Did the agent recognize and fix mistakes?
- **Stuck patterns** - Did the agent get stuck or loop?

### Tool Usage Analysis

Review tool calls and their results:

```bash
# View tool calls with outputs
jq 'select(.type == "function_call" or .type == "function_call_output")' session.jsonl
```

Evaluate:

1. **Appropriate tool selection** - Were the right tools used for the task?
2. **Correct parameters** - Were tools called with correct arguments?
3. **Result interpretation** - Did the agent correctly interpret tool outputs?
4. **Efficiency** - Were there redundant or unnecessary tool calls?
5. **Error handling** - How did the agent respond to tool failures?

### Common Issues to Identify

| Category | Examples |
|----------|----------|
| **Tool misuse** | Wrong tool for the job, incorrect parameters |
| **Missing tools** | Task needed a tool the CLI doesn't have |
| **Tool description issues** | Agent misunderstood what a tool does |
| **Knowledge gaps** | Missing context from AGENTS.md or system prompt |
| **Reasoning flaws** | Poor problem decomposition, missed steps |
| **Context loss** | Agent forgot earlier information |
| **Premature action** | Agent acted before understanding the task |
| **Over-caution** | Agent asked too many clarifying questions |
| **Under-caution** | Agent made assumptions without verification |

## Phase 4: Document Findings

### Recording Issues

Return a concise, evidence-backed report by default. Only create or update a project file when the user explicitly asks for a persistent report. If a file is requested, prefer a named artifact such as `session-analysis.md` and avoid inventing persistent issue trackers that are not already part of the repo.

```markdown
## [Date] Session: {session-id}

### Task
Brief description of what was asked.

### Outcome
- Completed / Partially completed / Failed
- Summary of what happened

### Issues Identified

#### Category: {issue-category}

**Description**: What went wrong and why.

**Evidence**: Specific examples from the session.

**Impact**: How this affects task completion.

**Recommendation**: Suggested fix.

### Patterns Observed
Any recurring patterns across sessions.

### Recommendations
Prioritized list of improvements.
```

### Issue Categories

- **tool_call_error** - Tool invocation failed or returned an unexpected error
- **tool_result_error** - Tool output was malformed, incomplete, too verbose, misleading, or hard to use
- **repeated_tool_call** - Same or equivalent call repeated without meaningful new information
- **permission_issue** - Sandbox, filesystem, approval, network, or OS permission handling blocked progress or was unclear
- **performance_issue** - Excessive duration, turn count, token use, output size, retries, or context growth
- **missing_context** - Needed project, environment, file, session, or prior-task context was absent
- **prompt_or_instruction_gap** - cake prompts, AGENTS.md, skills, or tool docs did not provide enough guidance or created ambiguity
- **instruction_following_issue** - Agent ignored or only partly followed user, developer, project, or tool instructions
- **missing_tool_or_capability** - An unavailable or poorly described tool, parser, permission path, or workflow would have materially helped
- **session_integrity_issue** - Malformed JSONL, missing task boundaries, unsupported format, missing records, orphan calls, duplicate ids, or incomplete final records

## Phase 5: Recommend Improvements

### Prioritization

Rank recommendations by impact:

1. **High impact** - Affects many tasks, significant quality degradation
2. **Medium impact** - Affects some tasks, moderate quality impact
3. **Low impact** - Affects few tasks, minor quality impact

### Improvement Areas

#### System Prompt Enhancements

- Add missing reasoning patterns or workflows
- Clarify how to handle common scenarios
- Improve instruction hierarchy and prioritization
- Add examples of good vs. bad behavior

#### Tool Descriptions

- Clarify when and how to use specific tools
- Add examples or common pitfalls
- Improve parameter descriptions
- Document tool limitations clearly

#### New Tools

- Identify capabilities that are missing
- Suggest tools for recurring patterns
- Consider compound operations

#### AGENTS.md Improvements

- Add missing build/test/lint commands
- Document patterns and conventions
- Include troubleshooting guidance
- Add project-specific context

#### Error Handling & Recovery

- Improve how the agent handles failures
- Add self-correction mechanisms
- Better context preservation

### Output Requirements

When evaluation is complete:

1. Summarize recommendations with rationale
2. Prioritize by impact level
3. Include concrete evidence: record type, line number when available, task id, timestamp, tool name and `call_id` for tool issues, and a short excerpt
4. Mention any test, log, or source-code checks used to validate the finding

## Quick Reference: Evaluation Checklist

### Session Analysis
- [ ] Locate and read the session
- [ ] Validate the first record is `session_meta` with `format_version: 4`
- [ ] Segment records by `task_start` and `task_complete`
- [ ] Identify the original task
- [ ] Trace tool calls and results
- [ ] Review reasoning messages
- [ ] Review `prompt_context` records for AGENTS.md, skills, environment, cwd, and date context
- [ ] Check final response for completion

### Quality Assessment
- [ ] Was the task completed?
- [ ] Was the solution correct?
- [ ] Were all requirements addressed?
- [ ] Was the approach efficient?

### Issue Identification
- [ ] Tool selection issues?
- [ ] Parameter problems?
- [ ] Result misinterpretation?
- [ ] Knowledge gaps?
- [ ] Reasoning flaws?
- [ ] Context loss?
- [ ] Error handling problems?

### Documentation
- [ ] Findings include evidence and implementation-ready recommendations
- [ ] Persistent report file created only if requested
- [ ] Recommendations prioritized

## File Locations

| File | Purpose |
|------|---------|
| `~/.local/share/cake/sessions/{uuid}.jsonl` | Session files (or `$CAKE_DATA_DIR/sessions/` if set) |
| `~/.cache/cake/cake.YYYY-MM-DD.log` | Daily logs (or `$CAKE_DATA_DIR/cake.YYYY-MM-DD.log` if set) |
