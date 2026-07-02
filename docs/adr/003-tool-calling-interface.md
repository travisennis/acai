---
status: accepted
date: 2026-07-02
decision-makers: Travis Ennis
---
# Tool Calling Interface

## Context

The acai-ts CLI tool needs a robust tool calling interface that supports multiple tools with different capabilities, proper input validation, error handling, and a clean abstraction for tool developers.

## Decision

### Tool Structure

Each tool follows a consistent structure with three key components:

```typescript
interface ToolObject<TInput> {
  toolDef: {
    description: string;
    inputSchema: z.ZodType<TInput>;
  };
  display(input: TInput): string;      // Formatted display for UI
  execute(input: TInput, options: ToolExecutionOptions): Promise<string>;
}
```

### Built-in Tools

| Tool | Purpose | Key Features |
|------|---------|--------------|
| `Read` | Read file contents | Encoding, line range, max lines |
| `Edit` | Edit files | Search/replace, validation |
| `Save` | Write files | Encoding, exists-check |
| `Bash` | Execute commands | Timeout, env vars, working dir |
| `Glob` | Pattern matching | Recursive, gitignore |
| `Grep` | Text search | Regex, context, case-sensitivity |
| `Ls` | List directories | Permissions, type filtering |
| `DirectoryTree` | Tree view | Max depth, results |
| `Think` | Model reflection | No-op passthrough |
| `Agent` | Sub-agent spawning | Tool filtering, model selection |
| `WebSearch` | Internet search | Provider-specific, num results |
| `WebFetch` | URL content | Jina cleaning, markdown output |
| `Skill` | Skill loading | Deduplication, args substitution |
| `ApplyPatch` | Patch application | Diff parsing, validation |

### Tool Initialization

Tools are initialized with workspace context and configuration:

```typescript
export async function initTools({ workspace }: { workspace: WorkspaceContext }) {
  const bashTool = await createBashTool({
    workspace,
    env: projectConfig.env,
  });

  const readFileTool = await createReadFileTool({ workspace });
  // ...
}
```

### AI SDK Integration

Tools are converted to AI SDK format for use with `streamText`:

```typescript
import { tool } from "ai";

export function toAiSdkTools(tools: CompleteToolSet, includeExecute = true) {
  return Object.fromEntries(
    Object.entries(tools).map(([name, toolObj]) => [
      name,
      tool({
        ...toolObj.toolDef,
        execute: includeExecute ? toolObj.execute : undefined,
      }),
    ])
  );
}
```

### Execution Flow

1. Model generates tool call with JSON input
2. Agent validates JSON structure before execution
3. Valid tools execute in parallel
4. Results collected and appended to message history
5. Model receives tool results for next iteration

### Error Handling

- Invalid JSON: Return error with truncated input preview
- Null/undefined input: Return schema validation error
- Execution errors: Catch and return formatted error message
- Abort signal: Respect cancellation from user interrupt

### Tool Repair

The agent can attempt to repair invalid tool inputs using a separate model call:

```typescript
experimental_repairToolCall: toolCallRepair(modelManager)
```

Failed tool calls are sent to a "tool-repair" model with the invalid input and expected schema. If repair succeeds, the corrected arguments are used.

## Consequences

### Positive
- Consistent interface across all tools
- Zod schema validation for type safety
- Parallel execution improves latency
- Built-in repair mechanism reduces model errors
- Clear separation of concerns (definition, display, execution)

### Negative
- All tools loaded into memory at startup
- No lazy loading or dynamic tool swapping
- Repair mechanism adds latency and cost

### Alternatives Considered

**Streaming Tool Results:** Instead of waiting for completion, streaming results would allow partial output. Not implemented due to complexity with parallel tool handling.

**Tool Categories:** Grouping tools into categories for selective enabling would be useful but not yet needed. May add later.

