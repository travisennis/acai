# Documentation

Quick reference for developing and extending acai.

## Getting Started

- [Installation & Setup](../CONTRIBUTING.md#development-environment-setup) - Local development environment
- [Usage Guide](usage.md) - Commands, keyboard shortcuts, prompt syntax
- [Project Structure](../ARCHITECTURE.md) - Source code organization and flow diagrams
- [Domain Model](DOMAIN.md) - Core concepts: session, agent, tool, skill, provider, prompt

## Configuration

- [Configuration Reference](configuration.md) - Environment variables, `acai.json`, project settings
- [Dynamic Tools](dynamic-tools.md) - Creating custom tools in `.acai/tools/`

## Extensibility

- [Skills System](skills.md) - Creating specialized instruction files for reusable workflows
- [Dynamic Tools](dynamic-tools.md) - Creating custom tools to extend acai's capabilities

## Development

- [AGENTS.md](../AGENTS.md) - Project-specific rules for AI assistants working in this repo
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Development setup, scripts, and code style
- [Architecture Overview](../ARCHITECTURE.md) - Internal architecture, modules, and flow diagrams
- [Agent Guardrails](guardrails/) - Focused rules by compatibility and risk surface

## Agent Guardrails

- [Agent Instructions](guardrails/agent-instructions.md)
- [API Stability and Compatibility](guardrails/api-stability-and-compatibility.md)
- [CLI and User Output](guardrails/cli-and-user-output.md)
- [Configuration](guardrails/configuration.md)
- [Dependencies, Build, CI, and Release](guardrails/dependencies-build-ci-release.md)
- [Documentation](guardrails/documentation.md)
- [Implementation Quality](guardrails/implementation-quality.md)
- [Performance and Resource Use](guardrails/performance-and-resource-use.md)
- [Persistence and Migrations](guardrails/persistence-and-migrations.md)
- [Security and Permissions](guardrails/security-and-permissions.md)
- [Testing and Verification](guardrails/testing-and-verification.md)

## Workflow Docs

- [Task Workflow](../AGENTS.md#managed-work-intake-with-ahm) - AHM task queue and lifecycle rules
- [Documentation Workflow](../docs/guardrails/documentation.md) - Documentation audit/update rules
- [Research Workflow](../AGENTS.md#managed-work-intake-with-ahm) - Research artifact rules
- [ExecPlans](../AGENTS.md#managed-work-intake-with-ahm) - Large-change planning format
- [ADR Workflow](adr/index.md) - Architecture decision records

## API Reference

### Source Modules

| Module | Purpose |
|--------|---------|
| `source/agent/` | Agent loop and sub-agent execution |
| `source/commands/` | REPL command implementations |
| `source/models/` | AI model providers and management |
| `source/tools/` | AI-callable tools (Bash, Read, Edit, Search, Web) |
| `source/tui/` | Terminal user interface components |
| `source/skills/` | Skills discovery and loading |
| `source/sessions/` | Session persistence and management |
