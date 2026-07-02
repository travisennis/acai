import type { ToolExecuteFunction, ToolSet } from "ai";
import { tool } from "ai";
import z from "zod";
import type { CompleteToolSet, CompleteTools } from "./index.ts";

export function toAiSdkTools(
  tools: CompleteToolSet,
  includeExecute = true,
): CompleteTools {
  return Object.fromEntries(
    Object.entries(tools).map(([name, toolObj]) => [
      name,
      tool({
        ...toolObj.toolDef,
        execute: (includeExecute
          ? toolObj.execute
          : undefined) as unknown as ToolExecuteFunction<
          unknown,
          string,
          unknown
        >,
      }),
    ]),
  ) as CompleteTools;
}

export function prepareTools(tools: ToolSet): {
  tools:
    | undefined
    | Array<{
        type: "function";
        function: {
          name: string;
          description: string | undefined;
          parameters: unknown;
        };
      }>;
} {
  const openaiCompatTools: Array<{
    type: "function";
    function: {
      name: string;
      description: string | undefined;
      parameters: unknown;
    };
  }> = [];

  for (const tool of Object.entries(tools)) {
    openaiCompatTools.push({
      type: "function",
      function: {
        name: tool[0],
        description:
          typeof tool[1].description === "string"
            ? tool[1].description
            : undefined,
        // biome-ignore lint/suspicious/noExplicitAny:  try to figure it out for now
        parameters: z.toJSONSchema(tool[1].inputSchema as any),
      },
    });
  }

  return { tools: openaiCompatTools };
}
