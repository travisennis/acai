import {
  generateText,
  isStepCount,
  NoSuchToolError,
  Output,
  type ToolCallRepairFunction,
  type ToolSet,
} from "ai";
import type z from "zod";
import { config } from "../config/index.ts";
import type { WorkspaceContext } from "../index.ts";
import { AiConfig } from "../models/ai-config.ts";
import type { ModelManager } from "../models/manager.js";
import type { PromptManager } from "../prompts/manager.ts";
import { systemPrompt } from "../prompts/system-prompt.ts";
import type { SessionManager } from "../sessions/manager.ts";
import { printExitSummary } from "../sessions/summary.ts";
import type { TokenCounter } from "../tokens/counter.ts";
import type { TokenTracker } from "../tokens/tracker.ts";
import type { CompleteToolSet } from "../tools/index.ts";
import { type CompleteTools, initTools } from "../tools/index.ts";
import { toAiSdkTools } from "../tools/utils.ts";
import { logger } from "../utils/logger.ts";

interface CliOptions {
  sessionManager: SessionManager;
  promptManager: PromptManager;
  modelManager: ModelManager;
  tokenTracker: TokenTracker;
  tokenCounter: TokenCounter;
  workspace: WorkspaceContext;
  noSession?: boolean;
}

export class Cli {
  private options: CliOptions;
  constructor(options: CliOptions) {
    this.options = options;
  }

  async run() {
    const { promptManager, modelManager, tokenTracker, sessionManager } =
      this.options;

    const abortController = new AbortController();
    const { signal } = abortController;

    const { cleanup } = this.setupSignalHandlers(abortController);

    const langModel = modelManager.getModel("cli");
    const modelConfig = modelManager.getModelMetadata("cli");

    const userPrompt = promptManager.get();

    const userMsg = promptManager.getUserMessage();

    sessionManager.appendUserMessage(userMsg);

    const { aiConfig, finalSystemPrompt, tools } =
      await this.initializeAiConfig({
        userPrompt,
        modelConfig,
      });

    try {
      const result = await generateText({
        model: langModel,
        maxOutputTokens: aiConfig.maxOutputTokens(),
        instructions: finalSystemPrompt,
        messages: sessionManager.get(),
        allowSystemInMessages: true,
        temperature: aiConfig.temperature(),
        topP: aiConfig.topP(),
        stopWhen: isStepCount(200),
        maxRetries: 2,
        providerOptions: aiConfig.providerOptions(),
        tools: toAiSdkTools(tools),
        // biome-ignore lint/style/useNamingConvention: third-party controlled
        experimental_repairToolCall:
          toolCallRepair<CompleteTools>(modelManager),
        abortSignal: signal,
      });

      if (result.responseMessages.length > 0) {
        sessionManager.appendResponseMessages(result.responseMessages);
      }

      // This tracks cumulative usage for every step in the generateText call.
      tokenTracker.trackUsage("cli", result.usage);

      if (!this.options.noSession) {
        await sessionManager.save();
      }

      process.stdout.end(
        result.text.endsWith("\n") ? result.text : `${result.text}\n`,
      );

      if (!sessionManager.isEmpty()) {
        printExitSummary(sessionManager, this.options.noSession);
      }

      cleanup();
      process.exit(0);
    } catch (e) {
      cleanup();

      const isAbortError =
        (e instanceof Error &&
          (e.name === "AbortError" ||
            e.message.includes("aborted") ||
            e.message.includes("No output generated"))) ||
        signal.aborted;

      if (isAbortError) {
        await this.handleAbort();
      } else {
        this.handleError(e);
      }
    }
  }

  private setupSignalHandlers(abortController: AbortController): {
    cleanup: () => void;
  } {
    const cb = abortController.abort.bind(abortController);
    process.on("SIGINT", cb);

    const cleanup = () => {
      process.removeListener("SIGINT", cb);
    };

    return { cleanup };
  }

  private async initializeAiConfig({
    userPrompt,
    modelConfig,
  }: {
    userPrompt: string;
    modelConfig: ReturnType<ModelManager["getModelMetadata"]>;
  }): Promise<{
    aiConfig: AiConfig;
    finalSystemPrompt: string;
    tools: CompleteToolSet;
  }> {
    const cliConfig = await config.getConfig();
    const finalSystemPromptResult = await systemPrompt({
      allowedDirs: this.options.workspace.allowedDirs,
      logsPath: cliConfig.logs?.path,
    });
    const finalSystemPrompt = finalSystemPromptResult.prompt;

    const aiConfig = new AiConfig({
      modelMetadata: modelConfig,
      prompt: userPrompt,
    });

    const tools = await initTools({
      workspace: this.options.workspace,
    });

    return { aiConfig, finalSystemPrompt, tools };
  }

  private async handleAbort(): Promise<void> {
    logger.info("CLI execution interrupted by user");
    if (!this.options.noSession) {
      try {
        await this.options.sessionManager.save();
      } catch {
        logger.warn("Failed to save message history on interrupt");
      }
    }
    process.exit(0);
  }

  private handleError(e: unknown): void {
    if (e instanceof Error) {
      logger.error(e);
    } else {
      logger.error(JSON.stringify(e, null, 2));
    }
    process.exit(1);
  }
}

const toolCallRepair = <T extends ToolSet>(modelManager: ModelManager) => {
  const fn: ToolCallRepairFunction<T> = async ({
    toolCall,
    tools,
    inputSchema,
    error,
  }) => {
    if (NoSuchToolError.isInstance(error)) {
      return null; // do not attempt to fix invalid tool names
    }
    const tool = tools[toolCall.toolName as keyof typeof tools];

    try {
      const { output: repairedArgs } = await generateText({
        model: modelManager.getModel("tool-repair"),
        output: Output.object({
          schema: tool.inputSchema as z.ZodType<unknown>,
        }),
        prompt: [
          `The model tried to call the tool "${toolCall.toolName}" but the input did not match the expected schema.`,
          "",
          "<invalid_input>",
          JSON.stringify(toolCall.input, null, 2),
          "</invalid_input>",
          "",
          "<expected_schema>",
          JSON.stringify(
            await inputSchema({ toolName: toolCall.toolName }),
            null,
            2,
          ),
          "</expected_schema>",
          "",
          "If any field is missing or undefined in the corrected input, you MUST explicitly set its value to null. Do NOT omit fields - every field in the schema must be present, even if with a null value.",
          "",
          "Return a corrected version of the input that conforms to the expected schema.",
        ].join("\n"),
      });

      return { ...toolCall, input: JSON.stringify(repairedArgs) };
    } catch (err) {
      logger.error(err, `Failed to repair tool call: ${toolCall.toolName}.`);
      return null;
    }
  };
  return fn;
};
