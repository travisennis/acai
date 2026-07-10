---
status: accepted
date: 2026-07-02
decision-makers: Travis Ennis
---
# Dynamic Tool Loading

## Context

The acai CLI tool needs a mechanism for users to define custom tools beyond the built-in set. These user-defined tools should be discoverable, executable, and consistent with the built-in tool interface.

## Decision

### Discovery Locations

Dynamic tools are discovered from two locations:

1. **Project**: `.acai/tools/` directory
2. **User**: `~/.acai/tools/` directory

Project tools take precedence over user tools when names conflict.

### Tool File Format

Dynamic tools are JavaScript files (`.js` or `.mjs`) that export two actions via environment variable:

```javascript
// my-tool.js

if (process.env.TOOL_ACTION === "describe") {
  // Return tool metadata as JSON
  console.log(JSON.stringify({
    name: "my-tool",
    description: "Does something useful",
    parameters: [
      { name: "input", type: "string", description: "Input text", required: true },
      { name: "count", type: "number", description: "Repeat count", required: false, default: 1 }
    ],
    needsApproval: true
  }));
  process.exit(0);
}

if (process.env.TOOL_ACTION === "execute") {
  // Read parameters from stdin
  const params = JSON.parse(require("fs").readFileSync(0, "utf-8"));

  // Execute logic
  const result = params.input.repeat(params.count);

  console.log(result);
  process.exit(0);
}
```

### Metadata Schema

```typescript
const toolMetadataSchema = z.object({
  name: z.string().regex(/^[a-zA-Z_][a-zA-Z0-9_-]*$/),
  description: z.string().min(1),
  parameters: z.array(
    z.object({
      name: z.string(),
      type: z.enum(["string", "number", "boolean"]),
      description: z.string().min(1),
      required: z.boolean().default(true),
      default: z.union([z.string(), z.number(), z.boolean()]).optional(),
    }),
  ),
  needsApproval: z.boolean().default(true),
});
```

### Loading Process

```typescript
export async function loadDynamicTools({
  baseDir,
  existingToolNames = [],
}: {
  baseDir: string;
  existingToolNames?: string[];
}) {
  const projectToolsDir = path.join(baseDir, ".acai", "tools");
  const userToolsDir = path.join(os.homedir(), ".acai", "tools");

  // Scan user first, then project
  await scanDir(userToolsDir, false);
  await scanDir(projectToolsDir, true);

  // Enforce maxTools limit
  if (toolMap.size > dynamicConfig.maxTools) {
    // Keep most recent (project entries)
  }

  // Create tool objects
  return tools;
}
```

### Schema Generation

Parameter schemas are generated from metadata using Zod:

```typescript
function generateZodSchema(parameters: ToolMetadata["parameters"]) {
  const fields: Record<string, z.ZodType> = {};

  for (const param of parameters) {
    let schema: z.ZodType;
    switch (param.type) {
      case "string":
        schema = z.string();
        break;
      case "number":
        schema = z.preprocess(/* coerce */, z.coerce.number().nullable());
        break;
      case "boolean":
        schema = z.preprocess(/* coerce */, z.coerce.boolean().nullable());
        break;
    }

    if (!param.required) schema = schema.optional();
    if (param.default !== undefined) schema = schema.default(param.default);

    fields[param.name] = schema.describe(param.description);
  }

  return z.object(fields);
}
```

### Execution Environment

Tool scripts run in a spawned Node.js process with:
- `TOOL_ACTION=execute` environment variable
- Parameters passed via stdin as JSON array
- `NODE_ENV=production` for production behavior
- 30-second timeout
- 2MB output limit

### Tool Registration

Dynamic tools are merged into the toolset after built-in tools:

```typescript
const tools = {
  // Built-in tools...
};

const dynamicTools = await loadDynamicTools({ baseDir: workspace.primaryDir });

// Merge, checking for conflicts
for (const [name, tool] of Object.entries(dynamicTools)) {
  if (existingToolNames.includes(name)) {
    // Skip, already defined
  } else {
    tools[name] = tool;
  }
}
```

### Approval Flow

Tools with `needsApproval: true` trigger a confirmation prompt before execution. This is checked in the tool's input processing, not enforced at the tool level.

## Consequences

### Positive
- Extensible without modifying core codebase
- Familiar JavaScript/Node.js execution environment
- Project and user tool directories for organization
- Automatic schema generation from metadata
- Conflict detection prevents overriding built-ins

### Negative
- Node.js dependency for all dynamic tools
- Security: arbitrary code execution (mitigated by approval flow)
- No type checking or validation at registration time
- Limited to JavaScript (could be extended to other runtimes)

### Alternatives Considered

**WASM-based tools:** Would provide sandboxing but adds complexity. Not implemented.

**YAML/JSON definition files:** Would be simpler but less flexible. JavaScript allows complex logic.

**HTTP-based tool services:** Would enable remote tools but requires server infrastructure. Not implemented.

