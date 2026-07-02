import type { AsyncReturnType } from "@travisennis/stdlib/types";
import type { ToolSet } from "ai";
import { config } from "../config/index.ts";
import type { WorkspaceContext } from "../index.ts";
import { ActivatedSkillsTracker } from "../skills/activated-tracker.ts";
import { ApplyPatchTool, createApplyPatchTool } from "./apply-patch.ts";
import { BashTool, createBashTool } from "./bash.ts";
import { loadDynamicTools } from "./dynamic-tool-loader.ts";
import { createEditFileTool, EditFileTool } from "./edit-file.ts";
import { createReadFileTool, ReadFileTool } from "./read-file.ts";
import { createSaveFileTool, SaveFileTool } from "./save-file.ts";
import { createSkillTool, SkillTool } from "./skill.ts";
import { createThinkTool, ThinkTool } from "./think.ts";
import { createWebFetchTool, WebFetchTool } from "./web-fetch.ts";
import { createWebSearchTool, WebSearchTool } from "./web-search.ts";

export type CompleteToolSet = {
  -readonly [K in keyof AsyncReturnType<typeof initTools>]: AsyncReturnType<
    typeof initTools
  >[K];
};

export type CompleteTools = ToolSet;

export type CompleteToolNames = keyof CompleteToolSet;

// Singleton tracker for activated skills, reset when a new session starts
const activatedSkillsTracker = new ActivatedSkillsTracker();

/**
 * Returns the activated skills tracker instance.
 * Used to reset the tracker when a new session starts.
 */
export function getActivatedSkillsTracker(): ActivatedSkillsTracker {
  return activatedSkillsTracker;
}

export async function initTools({
  workspace,
}: {
  workspace: WorkspaceContext;
}) {
  const readFileTool = await createReadFileTool({ workspace });

  const editFileTool = await createEditFileTool({ workspace });

  const saveFileTool = await createSaveFileTool({ workspace });

  const thinkTool = createThinkTool();

  const projectConfig = await config.getConfig();
  const bashTool = await createBashTool({ workspace, env: projectConfig.env });

  const skillTool = await createSkillTool(activatedSkillsTracker);

  const webSearchTool = await createWebSearchTool();

  const webFetchTool = await createWebFetchTool();

  const applyPatchTool = await createApplyPatchTool({ workspace });

  const dynamicTools = await loadDynamicTools({
    baseDir: workspace.primaryDir,
    existingToolNames: [
      ApplyPatchTool.name,
      EditFileTool.name,
      BashTool.name,
      SaveFileTool.name,
      ReadFileTool.name,
      ThinkTool.name,
      SkillTool.name,
      WebSearchTool.name,
      WebFetchTool.name,
    ],
    sessionContext: {
      sessionId: "",
      projectDir: workspace.primaryDir,
      agentName: "repl",
    },
  });

  // Build tools object for AI SDK
  const tools = {
    [ApplyPatchTool.name]: applyPatchTool,
    [EditFileTool.name]: editFileTool,
    [BashTool.name]: bashTool,
    [SaveFileTool.name]: saveFileTool,
    [ReadFileTool.name]: readFileTool,
    [ThinkTool.name]: thinkTool,
    [SkillTool.name]: skillTool,
    [WebSearchTool.name]: webSearchTool,
    [WebFetchTool.name]: webFetchTool,

    // Add dynamic tools - they already have toolDef structure
    ...Object.fromEntries(
      Object.entries(dynamicTools).map(([name, toolObj]) => [name, toolObj]),
    ),
  } as const;

  return tools;
}
