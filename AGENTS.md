# Agent Instructions

## Project

Acai is a TypeScript/Node.js CLI assistant for coding workflows, available as both one-shot CLI and interactive REPL/TUI. It integrates model providers, agent-callable tools, skills, dynamic tools, and persisted sessions.

Compatibility surfaces: CLI flags and commands, REPL/TUI output, prompt and AGENTS.md loading, project configuration, environment variables, dynamic tool and skill contracts, provider and model IDs, tool schemas, session and log formats, filesystem and shell permission boundaries, terminal rendering, package entry points, the supported Node.js range, and documented output formats. Preserve them unless explicitly changed.

## Operating Loop

1. Do managed-work intake first. If the request is about a task, ExecPlan, ADR, or research note, use `ahm` (see [Managed Work Intake With `ahm`](#managed-work-intake-with-ahm)) to understand that work item before choosing implementation docs. If the request is directly about code, CLI behavior, tests, docs, build, release, or repo mechanics, skip `ahm` intake and classify it directly.
2. Classify the concrete request before editing.
3. Load only the routed docs needed for that request.
4. Preserve compatibility surfaces unless explicitly changed.
5. Keep edits surgical and verify according to risk.
6. State the selected route and loaded docs, then handoff with changes, checks, and remaining risk.

When this file conflicts with a specialized workflow doc for that workflow, the specialized doc wins.
Keep AGENTS.md as routing, not as a command catalog or procedure manual.

## Workflow Routing

### CLI, REPL, TUI, And User Output

Use for command parsing, slash commands, stdin, interactive behavior, terminal rendering, markdown, autocomplete, and user-visible text.

Consult:

- [CLI and user output](docs/guardrails/cli-and-user-output.md), for flag, REPL, rendering, and help-text expectations.
- [Usage](docs/usage.md), for the documented commands and flags users rely on.
- [ARCHITECTURE.md](ARCHITECTURE.md), for the CLI and TUI boundaries.

Preserve documented commands, flags, exit behavior, and terminal-width handling.

### Agent Runtime, Tools, Skills, And Provider Contracts

Use for agent orchestration, model providers, AI SDK integration, tool calling, dynamic tools, skills, prompts, tokens, and middleware.

Consult:

- [API stability and compatibility](docs/guardrails/api-stability-and-compatibility.md), for agent, tool, and provider contract impact.
- [Security and permissions](docs/guardrails/security-and-permissions.md), for what a tool is allowed to do.
- [Dynamic tools](docs/dynamic-tools.md), for the dynamic tool contract.
- [Skills](docs/skills.md), for the skill contract and discovery.
- [ARCHITECTURE.md](ARCHITECTURE.md), for the runtime boundaries, and [DOMAIN.md](docs/DOMAIN.md), for the vocabulary these contracts use.
- [ADRs](docs/adr/), for prior decisions on model abstraction, tool calling, and sub-agents.

Keep tool schemas provider-compatible.

### Configuration, Environment, And Project Rules

Use for `.env`, `.acai/acai.json`, global and project config, config loading, AGENTS.md discovery, and generated rules.

Consult:

- [Configuration guardrail](docs/guardrails/configuration.md), for precedence, defaults, and secret handling.
- [Configuration reference](docs/configuration.md), for the documented keys and their meanings.
- [ARCHITECTURE.md](ARCHITECTURE.md), for where configuration resolves.

Preserve precedence, defaults, and secret handling.

### Persistence, Sessions, Logs, And File Formats

Use for session storage, resume/share/history, log paths, caches, selections, serialized records, and migrations.

Consult:

- [Persistence and migrations](docs/guardrails/persistence-and-migrations.md), for format-change and migration rules.
- [ADR 004](docs/adr/004-session-persistence-format.md), for the session persistence format.
- [ARCHITECTURE.md](ARCHITECTURE.md), for the persistence boundary.

Maintain backward compatibility unless the task explicitly scopes a migration.

### Security, Permissions, And Sandboxing

Use for shell execution, filesystem access, web fetch and search, dynamic tool execution, approvals, path validation, secrets, and log redaction.

Consult:

- [Security and permissions](docs/guardrails/security-and-permissions.md), for the permission model and approval boundaries.
- [Dynamic tools](docs/dynamic-tools.md), for the execution boundary of user-supplied tools.
- The security tests, which are the authority for enforced behavior.

Default to least privilege.

### Dependencies, Build, CI, And Release

Use for dependencies, Node and toolchain support, package metadata, build scripts, CI hooks, publishing, and release-adjacent changes.

Consult:

- [Dependencies, build, CI, and release](docs/guardrails/dependencies-build-ci-release.md), for dependency and release policy.
- [CONTRIBUTING.md](CONTRIBUTING.md), for the command catalog and verification expectations.
- `package.json`, which is the authority for entry points, scripts, and the supported Node range.

### Performance, Resource Use, And Large Outputs

Use for token budgets, streaming, process lifetime, stdout/stderr volume, file scans, cache behavior, and terminal rendering cost.

Consult:

- [Performance and resource use](docs/guardrails/performance-and-resource-use.md), for the budgets and bounding rules.
- [ARCHITECTURE.md](ARCHITECTURE.md), for where large data crosses a boundary.

Avoid unbounded reads, logs, model context, process output, and directory scans.

### Documentation And Workflow Artifacts

Use for README, architecture, guardrails, usage docs, ADRs, tasks, research, and ExecPlans.

Consult:

- [Documentation](docs/guardrails/documentation.md), for which surfaces require a doc update and where it belongs.
- The relevant `ahm context` output (`docs`, `task`, `research`, `plan`, `adr`), for managed work items.

Do not edit generated indexes by hand.

### Agent Instructions And Skills

Use for changes to this file, skills, or any other prose whose purpose is to change how an agent behaves.

Consult:

- [Agent-facing instructions](docs/guardrails/agent-instructions.md), for the evidence a behavior-shaping edit requires.

### Implementation Quality And Verification

Use for code changes, refactors, bug fixes, tests, and review readiness.

Consult:

- [Implementation quality](docs/guardrails/implementation-quality.md), for style and structural expectations.
- [Testing and verification](docs/guardrails/testing-and-verification.md), for which checks a change class requires.
- [CONTRIBUTING.md](CONTRIBUTING.md), for the commands themselves.
- [ARCHITECTURE.md](ARCHITECTURE.md), for the boundary a refactor must not cross.

Match existing style and scale checks to the changed surface.

### Managed Work Intake With `ahm`

`ahm` identifies and manages higher-order workflow records: tasks, ExecPlans, ADRs, and research. It is not an implementation route.

Run `ahm prime`, then the scoped `ahm context` command for the record type in question (`task`, `plan`, `adr`, `research`, or bare `ahm context` for a broad briefing). Treat that output as the canonical workflow guidance rather than restating it here. Then return to the routes above and load the docs the concrete work requires.

## Repository Rules

- Do not commit or push unless explicitly asked.
- Assume uncommitted changes may belong to the user.
- Do not revert, overwrite, or clean files you did not intentionally change.
- Inspect `git status --short` before broad edits.
- Report relevant remaining changes before handoff.
- Never hand-edit generated task, research, ExecPlan, or ADR indexes; update the source records and run the appropriate `ahm` command.

## Handoff

End with what changed, exact checks run, remaining risks or skipped checks, and actionable next steps. For commits, include hash, worktree cleanliness, and leftover changes.
