---
status: accepted
date: 2026-07-02
decision-makers: Travis Ennis
---
# Dynamic Tools Enhancement

## Context

ADR-009 established dynamic tool loading with JavaScript-only tools (`.js`/`.mjs`) using JSON metadata. This limited dynamic tools to Node.js scripts despite users wanting to write tools in bash, Python, and other languages. Additionally, the JSON metadata format is cumbersome for simple shell scripts, there was no scaffolding mechanism for new tools, and tools had no awareness of session context.

## Decision

### Language-Agnostic Tool Support

Dynamic tools now support any executable language, not just Node.js. The system detects how to run a tool through a resolution pipeline:

```typescript
function resolveToolInterpreter(scriptPath: string): InterpreterResult | null {
  // 1. Shebang detection: read first 256 bytes for #! line
  // 2. Extension mapping: .js/.mjs/.cjs → node, .sh/.bash → bash, .zsh → zsh, .py → python3, .rb → ruby
  // 3. Extensionless executables: check execute permission bit
  // 4. Unknown: skip with warning
}
```

Shebang parsing handles both direct paths (`#!/bin/bash`) and env-style (`#!/usr/bin/env python3`). Both `getMetadata()` and `spawnChildProcess()` now use the resolved interpreter instead of the hardcoded `process.execPath`.

### Text Schema Format (Amp-compatible)

A simpler text format is now supported alongside JSON for tool metadata. If JSON parsing of describe output fails, the text format is tried automatically:

```
name: run_tests
description: Run the tests in the project
workspace: string optional name of the workspace directory
test: string optional test name pattern
```

Format rules:
- `name` and `description` are required
- Parameter lines: `paramName: type [optional|required] description text`
- Supported types: `string`, `number`, `boolean`
- Default is required; `optional` must be explicit
- Lines starting with `#` or `//` are comments
- Tool name must match `/^[a-zA-Z_][a-zA-Z0-9_-]*$/`

Tools using text format receive key-value input on stdin during execute:

```
param1=value1
param2=value2
```

The format is tracked per-tool via a `ToolMetadataWithFormat` type (`json` | `text`) so the execution path knows which input format to use.

### .tool Companion Files

A `.tool` file contains only the text schema definition. The executable companion (same name without `.tool` extension) provides the implementation. For example:

- `.acai/tools/run_tests.tool` — text schema definition
- `.acai/tools/run_tests` — the executable bash script

When a `.tool` file is found, the system reads the schema from disk (no spawning needed) and locates the companion executable via `findCompanion()`, which checks known extensions and extensionless executables.

### Session Context Passing

Dynamic tools receive session context via environment variables during both describe and execute actions:

| Variable | Value | Description |
|----------|-------|-------------|
| `ACAI_SESSION_ID` | Current session UUID | Identifies the current session |
| `ACAI_PROJECT_DIR` | Primary workspace directory | The project root |
| `ACAI_AGENT_NAME` | Agent name (e.g., "repl") | The invoking agent |

A `SessionContext` type was added to `source/tools/types.ts`:

```typescript
export type SessionContext = {
  sessionId: string;
  projectDir: string;
  agentName: string;
};
```

Context is passed at load time through `loadDynamicTools()` and can be overridden at execution time via `ToolExecutionOptions.sessionContext`.

### CLI Scaffolding (`/tools` command)

A `/tools` REPL command was added with two subcommands:

- `/tools make <name> [--bash|--zsh|--node|--text] [--description <desc>] [--dir <path>]` — scaffolds a new dynamic tool with the appropriate template
- `/tools list` — lists discovered dynamic tools from both user and project directories

Templates generate working tool files with proper shebangs, describe/execute protocol handling, and parameter reading.

## Consequences

### Positive
- Bash, Python, Ruby, and other scripting languages can now be used for dynamic tools without Node.js dependency
- Text schema format lowers the barrier for simple shell script tools
- `.tool` companion files separate schema from implementation, enabling schema-only definitions
- Session context allows tools to behave differently based on project directory or session
- `/tools make` eliminates the blank-page problem for new tool authors
- Existing `.js`/`.mjs` tools continue to work unchanged

### Negative
- Shebang detection adds filesystem I/O (reading first 256 bytes) during tool discovery
- Text format parsing is less strict than JSON schema validation; malformed schemas may produce confusing errors
- `.tool` companion files add a two-file pattern that must be kept in sync
- Extensionless executable detection relies on filesystem permission bits, which may not be preserved across all version control systems

### Alternatives Considered

**WASM-based tools (from ADR-009):** Still not implemented. Language-agnostic support is simpler and more practical.

**JSON-only metadata with wrapper scripts:** Would require users to write JSON metadata even for bash tools. The text format is more ergonomic.

**Separate schema files in YAML/JSON:** The `.tool` text format was chosen over YAML/JSON schema files because it matches Amp's convention and is simpler for bash tool authors.

