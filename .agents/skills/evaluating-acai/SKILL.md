---
name: evaluating-acai
description: Evaluate acai CLI session performance to identify issues and recommend improvements. Use this skill when asked to evaluate, assess, review, or analyze how well acai performed on a task, or when investigating session quality for improvements to the project.
---

# Evaluating Acai CLI Sessions

This skill guides systematic evaluation of acai CLI sessions to identify issues and recommend improvements.

## When to Use This Skill

- After completing a task with acai to evaluate performance
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

Before evaluating, understand what tools the acai CLI has available. This ensures evaluation is based on actual capabilities, not assumptions.

1. Read these files:
   - `ARCHITECTURE.md` - Overall architecture
   - `src/tools/mod.rs` - Available tools
   - `src/tools/*.rs` - Tool capabilities and parameters

2. Key distinction: **You have different tools than the acai CLI**. Only identify issues valid for the CLI's actual toolset.

## Phase 1: Locate and Read the Session

### Finding Sessions

Sessions are stored in `~/.cache/acai/sessions/` organized by working directory hash:

```bash
# Find session directory for current project
echo -n "$(pwd)" | shasum -a 256 | cut -c1-16

# Or list all session directories
ls ~/.cache/acai/sessions/

# Find latest session for a directory
ls -la ~/.cache/acai/sessions/*/latest
```

### Reading Sessions

Use the `ReadSession` tool for token-efficient reading:

```
ReadSession {
    session_id: "04cddcba-3dd0-43f7-811c-829a5b0b9e87",
    max_turns: 50
}
```

Or read the raw JSONL file:

```bash
# View full session
jq '.' ~/.cache/acai/sessions/{hash}/{uuid}.jsonl

# View last 10 messages (most relevant for evaluation)
tail -10 ~/.cache/acai/sessions/{hash}/{uuid}.jsonl | jq '.'

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

# Count message types
jq -r '.type' session.jsonl | sort | uniq -c
```

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

Append findings to `current-issues.md` with this structure:

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

- **System prompt** - Core behavior or reasoning flaws
- **Tool gaps** - Missing tools that would help
- **Tool descriptions** - Unclear or misleading tool docs
- **Knowledge gaps** - Missing context in AGENTS.md
- **Capability limitations** - General abilities lacking
- **Error handling** - Poor failure recovery
- **Context management** - Information loss or overload

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

1. Update `current-issues.md` with findings
2. Record learnings in `usage.md` if patterns emerge
3. Summarize recommendations with rationale
4. Prioritize by impact level

## Quick Reference: Evaluation Checklist

### Session Analysis
- [ ] Locate and read the session
- [ ] Identify the original task
- [ ] Trace tool calls and results
- [ ] Review reasoning messages
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
- [ ] Findings recorded in `current-issues.md`
- [ ] Patterns noted in `usage.md`
- [ ] Recommendations prioritized

## File Locations

| File | Purpose |
|------|---------|
| `~/.cache/acai/sessions/{hash}/{uuid}.jsonl` | Session files |
| `~/.cache/acai/sessions/{hash}/latest` | Symlink to latest session |
| `current-issues.md` | Documented issues |
| `usage.md` | Usage patterns and learnings |