# Agent Instructions

## Project
Acai is a TypeScript/Node.js CLI assistant for coding workflows, available as both one-shot CLI and interactive REPL/TUI. It integrates model providers, agent-callable tools, skills, dynamic tools, and persisted sessions.

Compatibility surfaces: CLI flags/commands, REPL/TUI output, prompt and AGENTS.md loading, `acai.json`, environment variables, dynamic tool and skill contracts, provider/model IDs, tool schemas, session/log formats, filesystem and shell permission boundaries, terminal rendering, package entry points, Node.js >=24, and documented output formats. Preserve them unless explicitly changed.

## Operating Loop
1. Do managed-work intake first. If the request is about a task, ExecPlan, ADR, or research note, use `ahm` (see Managed Work Intake With `ahm`) to understand that work item before choosing implementation docs. If the request is directly about code, CLI behavior, tests, docs, build, release, or repo mechanics, skip `ahm` intake and classify it directly.
2. Classify the concrete request before editing.
3. Load only the routed docs needed for that request.
4. Preserve compatibility surfaces unless explicitly changed.
5. Keep edits surgical and verify according to risk.
6. State the selected route and loaded docs, then handoff with changes, checks, and remaining risk.

When this file conflicts with a specialized workflow doc for that workflow, the specialized doc wins.
Keep AGENTS.md as routing, not as a command catalog or procedure manual.

## Workflow Routing

### CLI, REPL, TUI, And User Output
Use this workflow for command parsing, slash commands, stdin, interactive behavior, terminal rendering, markdown, autocomplete, and user-visible text. Consult `docs/guardrails/cli-and-user-output.md`, `docs/usage.md`, and `ARCHITECTURE.md`. Preserve documented commands, flags, exit behavior, and terminal-width handling.

### Agent Runtime, Tools, Skills, And Provider Contracts
Use this workflow for agent orchestration, model providers, AI SDK integration, tool calling, dynamic tools, skills, prompts, tokens, and middleware. Consult `docs/guardrails/api-stability-and-compatibility.md`, `docs/guardrails/security-and-permissions.md`, `docs/dynamic-tools.md`, `docs/skills.md`, `ARCHITECTURE.md`, and `docs/adr/`. Keep tool schemas provider-compatible.

### Configuration, Environment, And Project Rules
Use this workflow for `.env`, `acai.json`, config loading, AGENTS.md discovery, global/project settings, and generated rules. Consult `docs/guardrails/configuration.md`, `docs/configuration.md`, and `ARCHITECTURE.md`. Preserve precedence, defaults, and secret handling.

### Persistence, Sessions, Logs, And File Formats
Use this workflow for session storage, resume/share/history, log paths, caches, selections, serialized records, and migrations. Consult `docs/guardrails/persistence-and-migrations.md`, `ARCHITECTURE.md`, and `docs/adr/004-session-persistence-format.md`. Maintain backward compatibility unless the task explicitly scopes a migration.

### Security, Permissions, And Sandboxing
Use this workflow for shell execution, filesystem access, web fetch/search, dynamic tool execution, approvals, path validation, secrets, and log redaction. Consult `docs/guardrails/security-and-permissions.md`, `docs/dynamic-tools.md`, and security tests. Default to least privilege.

### Dependencies, Build, CI, And Release
Use this workflow for dependencies, Node/toolchain support, package metadata, build scripts, CI hooks, publishing, and release-adjacent changes. Consult `docs/guardrails/dependencies-build-ci-release.md`, `CONTRIBUTING.md`, and `package.json`. Preserve Node.js >=24 and package entry points.

### Performance, Resource Use, And Large Outputs
Use this workflow for token budgets, streaming, process lifetime, stdout/stderr volume, file scans, cache behavior, and terminal rendering cost. Consult `docs/guardrails/performance-and-resource-use.md` and `ARCHITECTURE.md`. Avoid unbounded reads, logs, model context, process output, and directory scans.

### Documentation And Workflow Artifacts
Use this workflow for README, architecture, guardrails, usage docs, ADRs, tasks, research, and ExecPlans. Consult `docs/guardrails/documentation.md` and, for managed work items, the relevant `ahm context` output (`docs`, `task`, `research`, `plan`, `adr`). Do not edit generated indexes by hand.

### Implementation Quality And Verification
Use this workflow for code changes, refactors, bug fixes, tests, and review readiness. Consult `docs/guardrails/implementation-quality.md`, `docs/guardrails/testing-and-verification.md`, `CONTRIBUTING.md`, and `ARCHITECTURE.md`. Match existing style and scale checks to the changed surface.

### Managed Work Intake With `ahm`
`ahm` is for understanding and managing higher-order workflow records (tasks,
ExecPlans, ADRs, research). It is not the implementation route. These overlays do
not replace the specific workflow routes above: use `ahm` first to identify or
manage the work item, then re-classify the concrete task and load the relevant
routed workflow docs before editing. Treat `ahm context` output as the canonical
workflow guidance.

- Tasks: run `ahm context task`, inspect the task with `ahm task ...`, open the
  task file, then return to Workflow Routing and choose the route(s) required by
  the task content.
- ExecPlans: run `ahm context plan` when the request or task calls for an ExecPlan.
- ADRs: run `ahm context adr` when the request or task calls for an ADR, and use
  `ahm adr ...` for lifecycle changes.
- Research: run `ahm context research`, then use `.agents/.research/index.md` as
  the map, when asked to create, update, organize, or use research.
- General briefing: run `ahm context` only when asked for broad project context
  or when no narrower managed-work context applies.

## Repository Rules
- Do not commit or push unless explicitly asked.
- Assume uncommitted changes may belong to the user.
- Do not revert, overwrite, or clean files you did not intentionally change.
- Inspect `git status --short` before broad edits.
- Update `ARCHITECTURE.md` when adding/removing files or moving implementation.
- Update `README.md` when adding/removing user-visible features or docs.
- Report relevant remaining changes before handoff.
- Never hand-edit generated task, research, ExecPlan, or ADR indexes; update the source records and run the appropriate `ahm` command (`ahm task ...` for task state, `ahm adr ...` for ADR lifecycle).

## Handoff
End with what changed, exact checks run, remaining risks or skipped checks, and actionable next steps. For commits, include hash, worktree cleanliness, and leftover changes.
