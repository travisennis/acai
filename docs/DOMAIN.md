# Acai Domain

## What Acai Is

Acai is an AI-assisted coding tool that runs in the terminal. It connects large language models (LLMs) to a developer's codebase so the model can read, edit, search, and reason about code in context. It is not an IDE plugin — it is a standalone CLI with both one-shot and interactive modes.

## Core Concepts

### Session

A **session** is one conversation between the user and the AI agent. It begins when the user starts acai (or resumes a previous one) and ends when the user exits. Sessions are persisted as JSON files in `~/.acai/sessions/` and can be resumed later via `--resume` or `--continue`. Each session carries its message history, token usage, and timing telemetry.

### Agent

The **agent** is the AI loop that orchestrates a model, a set of tools, and the session. It receives a user prompt, generates a model response (possibly calling tools), streams the result, and repeats until the task is done. Acai runs one agent per session. There is no multi-agent orchestration — the sub-agent system was removed (see ADR-012).

### Tool

A **tool** is a capability the agent can invoke during a conversation — reading a file, running a shell command, searching the web, etc. Tools are defined in `source/tools/` with Zod schemas and execute in a sandboxed environment. The agent does not call tools directly — it emits tool calls that the agent loop routes and executes.

### Built-in Tool vs Dynamic Tool

- **Built-in tools** ship with acai: Bash, BashSession, Read, Edit, ApplyPatch, SaveFile, WebSearch, WebFetch, Think, Skill. They are always available.
- **Dynamic tools** are user-defined scripts placed in `.acai/tools/`. They can be written in any language (bash, Python, Node.js, etc.) and follow a describe/execute protocol. Dynamic tools are loaded at startup and cannot conflict with built-in tool names.

### Skill

A **skill** is a specialized instruction file (`SKILL.md` with YAML frontmatter) that tells the agent how to handle a specific kind of task. Skills are discovered at startup from several directories (`~/.agents/skills/`, `.claude/skills/`, etc.) and loaded on demand when the agent determines the skill matches the task. A skill is not a tool — it is documentation-with-workflow that the agent reads and follows.

### Model Provider

A **model provider** is an external service that exposes an LLM API. Acai supports Anthropic, OpenAI, Google, Groq, DeepSeek, xAI, OpenRouter, OpenCode Zen, and OpenCode Go. Each provider is implemented as a separate module in `source/models/`. The AI SDK (`ai` package) provides the unified interface.

### Prompt

A **prompt** is the combined input sent to the model each turn. It consists of:
- A **system prompt** generated from `AGENTS.md` files (user-level, global, and project-level) plus environment context.
- The **conversation history** from the session.
- The **current user message** with mention expansions (`@file`, `!command`).

### Mention

A **mention** is a shorthand in user input: `@filename` includes a file's contents, `@dirname` includes a directory tree, `` !`command` `` runs a shell command and includes its output. Mentions are expanded before the prompt is sent to the model.

## Relationship Summary

```
User types prompt (with @mentions, !commands)
       │
       ▼
    Agent loop
       │
       ├──► Model provider (Anthropic, OpenAI, etc.)
       │       └── responds with text + optional tool calls
       │
       ├──► Tool execution (built-in or dynamic)
       │       └── returns result to agent
       │
       ├──► Skill loading (on-demand, if task matches)
       │       └── agent reads instructions
       │
       └──► Session persistence (saves to ~/.acai/sessions/)
               └── can be resumed later
```

## Key Distinctions

| Concept | What it is | Example |
|---------|-----------|---------|
| Tool | An action the agent can take | Read a file, run bash |
| Skill | Instructions the agent follows | "How to extract PDF text" |
| Session | A conversation record | One chat from start to exit |
| Prompt | Input sent to the model each turn | System + history + current message |
| Provider | An LLM API service | Anthropic Claude, OpenAI GPT |
| Dynamic Tool | A user-written script called as a tool | `.acai/tools/run-tests` |
