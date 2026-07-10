# Contributing to Acai

## Development Environment Setup

This project requires **Node.js ≥24** (pinned in `.node-version` — `fnm`, `nvm`, and agents pick this up automatically).

### Quick start (single command)

```bash
git clone https://github.com/travisennis/acai.git
cd acai
npm run setup
```

`npm run setup` checks prerequisites (Node version, Git, Ripgrep), installs dependencies, and creates the `~/.acai/` data directory.

### Manual walkthrough

1. **Clone and install dependencies:**
   ```bash
   git clone https://github.com/travisennis/acai.git
   cd acai
   npm install
   ```

2. **Set up environment variables:**
   ```bash
   touch .env
   # Add your API keys (see docs/configuration.md)
   ```

### Development workflow

```bash
# Run in development mode (uses .env file)
npm run dev

# Build and test
npm run build
npm test

# Code quality
npm run lint
npm run format
```

Interactive REPL testing should run in tmux. See
`.agents/skills/manual-testing/SKILL.md` for the project workflow.

Application logs live at `~/.acai/logs/current.log`; use `tail` instead of
reading the full file. Session files live at `~/.acai/sessions/*.json`; use a
session-reading helper when available rather than loading large JSON files
directly.

## Available NPM Scripts

| Script | Description |
| :--- | :--- |
| `npm run build` | Compiles TypeScript source to JavaScript. |
| `npm run clean` | Removes `dist/` and `coverage/` directories. |
| `npm run compile` | Compiles TypeScript files (`tsc --pretty`). |
| `npm run lint` | Runs Biome linter for code style and quality. |
| `npm run lint:fix` | Automatically fixes linting issues using Biome. |
| `npm run test` | Runs unit tests using `node:test`. |
| `npm run test:coverage` | Runs tests with code coverage using `c8`. |
| `npm run format` | Formats the codebase using Biome. |
| `npm run dev` | Starts the application in development mode. |
| `npm run check` | Runs typecheck, lint fix, and format. |
| `npm run typecheck` | Type checks the codebase without emitting files. |
| `npm run oxlint` | Runs Oxlint for additional code quality checks. |
| `npm run knip` | Detects unused files, dependencies, and exports. |
| `npm run update` | Interactively checks for outdated npm packages. |
| `npm run cpd` | Checks for copy-pasted code using `jscpd`. |

For one focused test file, use:

```bash
node --no-warnings --require ./test/setup.js --test test/path/to/file.test.ts
```

## Verification Expectations

- Use focused tests while iterating.
- Run `npm run typecheck` after type-heavy changes.
- Run `npm run build` for package entry point, compiler, or publish-related
  changes.
- Run `npm run check` when completing code, config, dependency, fixture, or
  template changes.
- Docs-only handoff does not require full CI unless the change affects code,
  config, generated artifacts, tested examples, or workflow metadata.
- Before committing, always run `npm run check`.

If `npm install` or typecheck fails because type definitions or `node_modules`
are stale, remove `node_modules` and reinstall with:

```bash
npm install --include=dev --ignore-scripts
```

## Code Style

- **Language:** Strict TypeScript with ESNext target
- **Modules:** ES Modules only. Use `.ts` extensions for relative imports
- **Node Built-ins:** Use `node:` prefix (e.g., `import fs from 'node:fs'`)
- **Formatting:** Biome rules (2-space indents, 80 char line width)
- **Types:** Explicit types required. Avoid `any`. No non-null assertions (`!`)
- **Naming:** camelCase (variables/functions), PascalCase (classes/types)
- **Testing:** `node:test` and `node:assert/strict` in `./test` directory
- **Comments:** Add comments only to clarify non-obvious behavior. Do not add
  comments explaining that an edit removed or changed code.
- **Tool Schemas:** For agent/LLM tool schemas, do not use `.optional()` for
  fields sent to OpenAI-compatible providers unless the field may truly be
  omitted from generated JSON Schema. Many compatible endpoints expect every
  tool field in `required`; use nullable schemas with `.default(null)` for
  omitted-at-runtime values that should still be required and nullable.

## Commit & Branch Strategy

- **Commits:** Follow [Conventional Commits](https://www.conventionalcommits.org/) standard
- **Message Length:** Keep lines under 100 characters
- **Branch Strategy:** Feature branches (`feat/`, `fix/`) merged to `master`
- **Special characters:** Use single quotes around commit messages containing
  shell-special characters.

Recommended scopes aligned with the repository architecture:

| Scope | Description |
| :--- | :--- |
| `cli` | Command-line interface and argument parsing |
| `agent` | Agent orchestration, conversation loop, tool execution |
| `tools` | Tool definitions such as Bash, Read, Edit, and Write |
| `config` | Configuration, sessions, and data directory behavior |
| `session` | Session persistence and management |
| `model` | Model configuration and API types |
| `prompts` | System prompt construction and AGENTS.md integration |
| `logger` | Logging configuration |

## PR Requirements

- All tests must pass
- Code follows style guidelines
- No new warnings generated
- Documentation updated if needed
