---
status: superseded by ADR-012
date: 2026-07-02
superseded-by: 012
decision-makers: Travis Ennis
---
# Sub-agent Communication

## Context

The acai CLI tool needs a sub-agent system where specialized agents can be spawned to handle specific tasks while passing context back to the parent agent. Sub-agents need access to workspace context, tool filtering, and proper result aggregation.

## Decision

### Agent Tool

The `Agent` tool allows spawning sub-agents from within a conversation:

```typescript
const inputSchema = z.object({
  prompt: z.string().describe("The task to delegate to the sub-agent"),
  model: z.string().optional().describe("Model to use (default: inherit)"),
  tools: z.array(z.string()).optional().describe("Tool names to allow"),
});
```

### Sub-agent Lifecycle

```
┌─────────────────────────────────────────────────────────────┐
│ Parent Agent                                                │
│  - Receives user prompt                                     │
│  - Determines delegation needed                             │
│  - Spawns Agent tool call                                   │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │ Sub-agent                                            │   │
│  │  - Inherits parent context (workspace, tools)         │   │
│  │  - Runs independently with filtered tool set          │   │
│  │  - Returns result to parent                          │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                              │
│  - Receives sub-agent result                                │
│  - Continues parent task                                    │
└─────────────────────────────────────────────────────────────┘
```

### Context Passing

Sub-agents receive context from the parent:

```typescript
async function createAgentTools({
  workspace,
}: {
  workspace: WorkspaceContext;
}) {
  return {
    Agent: {
      toolDef: {
        description: "Delegate a task to a specialized sub-agent...",
        inputSchema,
      },
      display({ prompt }: AgentInput) {
        return `Delegating: ${prompt.slice(0, 50)}...`;
      },
      async execute({ prompt, model, tools }, options) {
        const subAgent = new SubAgent({
          workspace,
          // Pass relevant context
        });

        const result = await subAgent.run({ prompt, tools });
        return result;
      },
    },
  };
}
```

### Tool Filtering

Sub-agents can be restricted to specific tools:

```typescript
const tools = await initTools({ workspace });
const filteredTools = toolsToUse
  ? Object.fromEntries(
      toolsToUse.map((name) => [name, tools[name]])
    )
  : tools;
```

### Sub-agent Implementation

The `SubAgent` class handles the delegation:

```typescript
export class SubAgent {
  constructor({ workspace }: { workspace: WorkspaceContext });

  async run({
    prompt,
    model,
    tools,
  }: {
    prompt: string;
    model?: string;
    tools?: string[];
  }): Promise<string>;
}
```

### Result Aggregation

Sub-agent results are returned as formatted text:

```
--- Sub-agent Result ---

The sub-agent completed the following tasks:

1. Analyzed codebase structure
2. Identified 3 potential improvements
3. Generated implementation plan

--- End Sub-agent Result ---
```

## Consequences

### Positive
- Task decomposition allows parallel work
- Tool filtering prevents sub-agents from overreaching
- Independent model selection optimizes cost/quality
- Clean separation between parent and child context

### Negative
- Context passing can be lossy
- Nested sub-agents increase complexity
- Token costs multiply with delegation depth

### Alternatives Considered

**Message Passing:** Instead of text results, structured message passing between agents would be more robust. Not implemented due to complexity. May revisit if needs grow.

**Shared History:** Sub-agents could append directly to parent session history. Currently they return results instead, keeping history cleaner.

## Supersession

Superseded by [ADR-012](012-remove-specialized-search-and-agent-tools.md).
