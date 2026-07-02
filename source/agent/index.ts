import type {
  LanguageModelUsage,
  ToolCallRepairFunction,
  ToolExecuteFunction,
  ToolModelMessage,
  ToolSet,
} from "ai";
import {
  generateText,
  InvalidToolInputError,
  NoOutputGeneratedError,
  NoSuchToolError,
  Output,
  streamText,
} from "ai";
import type z from "zod";
import type { Config } from "../config/index.ts";
import { AiConfig } from "../models/ai-config.ts";
import type { ModelManager } from "../models/manager.ts";
import type { ModelMetadata } from "../models/providers.ts";
import type { SessionManager } from "../sessions/manager.ts";
import type { TokenTracker } from "../tokens/tracker.ts";
import type {
  CompleteToolNames,
  CompleteToolSet,
  CompleteTools,
} from "../tools/index.ts";
import { toAiSdkTools } from "../tools/utils.ts";
import { logger } from "../utils/logger.ts";

/**
 * Whether a stream chunk carries model output (used to mark time-to-first-token).
 */
function isContentChunk(chunk: { type?: string }): boolean {
  return (
    chunk.type === "text-delta" ||
    chunk.type === "reasoning-delta" ||
    chunk.type === "tool-call"
  );
}

/**
 * Rough input-token estimate for the request.start event. Real input token
 * counts are reported on request.end from provider usage.
 */
function estimateInputTokens(
  messages: unknown[],
  systemPrompt: string,
): number {
  const chars = systemPrompt.length + JSON.stringify(messages).length;
  return Math.ceil(chars / 4);
}

/**
 * Selects provider response headers worth keeping for telemetry: request id,
 * rate-limit headers, and retry hints. Header names vary per provider.
 */
function extractProviderHeaders(
  headers: Record<string, string> | undefined,
): Record<string, string> {
  if (!headers) {
    return {};
  }
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(headers)) {
    const lower = key.toLowerCase();
    if (
      lower === "x-request-id" ||
      lower === "retry-after" ||
      lower.includes("ratelimit") ||
      lower.includes("rate-limit")
    ) {
      out[lower] = value;
    }
  }
  return out;
}

type AgentOptions = {
  config: Config;
  modelManager: ModelManager;
  tokenTracker: TokenTracker;
  sessionManager: SessionManager;
};

type RunOptions = {
  systemPrompt: string;
  input: string;
  tools: CompleteToolSet;
  activeTools?: CompleteToolNames[];
  abortSignal?: AbortSignal;
  maxRetries?: number;
  maxIterations?: number;
};

export type ToolEvent =
  | {
      type: "tool-call-start";
      name: string;
      toolCallId: string;
      msg: string;
      args: unknown;
    }
  | {
      type: "tool-call-end";
      name: string;
      toolCallId: string;
      msg: string;
      args: unknown;
    }
  | {
      type: "tool-call-error";
      name: string;
      toolCallId: string;
      msg: string;
      args: unknown;
    };

type ToolCallLifeCycle = {
  type: "tool-call-lifecycle";
  toolCallId: string;
  events: ToolEvent[];
};

export type AgentEvent =
  // Agent lifecycle
  | { type: "agent-start" }
  | { type: "agent-stop" }
  | { type: "agent-error"; message: string }
  // Step lifecycle
  | { type: "step-start" }
  | { type: "step-stop" }
  // Thinking and message streaming
  | { type: "thinking-start"; content: string }
  | { type: "thinking"; content: string }
  | { type: "thinking-end"; content: string }
  | { type: "message"; role: "user"; content: string }
  | { type: "message-start"; role: "assistant"; content: string }
  | { type: "message"; role: "assistant"; content: string }
  | { type: "message-end"; role: "assistant"; content: string }
  // Tool execution lifecycle
  | ToolCallLifeCycle;

// export interface AgentState {
// systemPrompt: string;
// model: Model<any>;
// thinkingLevel: ThinkingLevel;
// tools: AgentTool<any>[];
// messages: AppMessage[]; // Can include attachments + custom message types
// isStreaming: boolean;
// streamMessage: Message | null;
// pendingToolCalls: Set<string>;
// error?: string;
// }

type ModelUsage = {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  cachedInputTokens: number;
  reasoningTokens: number;
  inputTokenDetails: {
    noCacheTokens: number;
    cacheReadTokens: number;
    cacheWriteTokens: number;
  };
  outputTokenDetails: {
    textTokens: number;
    reasoningTokens: number;
  };
};

interface PendingToolCall {
  call: {
    toolName: string;
    toolCallId: string;
    input: unknown;
    invalid?: boolean;
    error?: unknown;
  };
  toolName: string;
  // biome-ignore lint/suspicious/noExplicitAny: tool type from CompleteTools
  iTool: any;
}

export type AgentState = {
  modelId: string;
  modelConfig: ModelMetadata;
  steps: {
    toolResults: Array<{ toolName: string }>;
    toolCalls: Array<{ toolName: string }>;
  }[];
  usage: ModelUsage;
  totalUsage: ModelUsage;
  timestamps: {
    start: number;
    stop: number;
  };
};

export class Agent {
  private opts: AgentOptions;
  private config: Config;
  private _state: AgentState;
  private abortController: AbortController;

  constructor(opts: AgentOptions) {
    this.opts = opts;
    this.config = opts.config;
    this.abortController = new AbortController();
    this._state = this.resetState();
  }

  setConfig(config: Config) {
    this.config = config;
  }

  get state() {
    return this._state;
  }

  get abortSignal() {
    return this.abortController.signal;
  }

  // biome-ignore lint/complexity/noExcessiveCognitiveComplexity: will be refactored in a future task
  async *run(args: RunOptions): AsyncGenerator<AgentEvent> {
    yield {
      type: "agent-start",
    };

    const { systemPrompt, input, tools, activeTools, abortSignal } = args;

    this.resetState();

    this._state.timestamps.start = performance.now();

    const { modelManager, sessionManager } = this.opts;
    const maxIterations =
      args.maxIterations ?? this.config.loop.maxIterations ?? 200;
    const maxRetries = args.maxRetries ?? 2;

    const langModel = modelManager.getModel("repl");
    const modelConfig = modelManager.getModelMetadata("repl");

    const aiConfig = new AiConfig({
      modelMetadata: modelConfig,
      prompt: input,
    });

    let iter = 0;
    let consecutiveErrors = 0;
    let hasEmittedTerminalEvent = false;
    while (iter < maxIterations) {
      if (abortSignal?.aborted) {
        logger.warn("The agent loop was aborted by the user.");
        yield {
          type: "agent-stop",
        };
        hasEmittedTerminalEvent = true;
        break;
      }

      yield {
        type: "step-start",
      };

      const toolsCalled: Map<string, ToolEvent[]> = new Map();

      const requestId = `${sessionManager.getSessionId()}:${iter}`;
      const requestStart = performance.now();
      let firstTokenAt: number | null = null;

      try {
        // Check abort signal again before starting streamText
        if (abortSignal?.aborted) {
          throw new Error("Agent aborted before streamText");
        }

        const messages = sessionManager.get();
        const aiSdkTools = toAiSdkTools(tools, false);

        logger.info(
          {
            event: "model.request.start",
            requestId,
            sessionId: sessionManager.getSessionId(),
            model: modelConfig.id,
            provider: modelConfig.provider,
            iteration: iter,
            inputTokenEstimate: estimateInputTokens(messages, systemPrompt),
          },
          "model.request.start",
        );

        const result = streamText({
          model: langModel,
          maxOutputTokens: aiConfig.maxOutputTokens(),
          instructions: systemPrompt,
          messages,
          allowSystemInMessages: true,
          temperature: aiConfig.temperature(),
          topP: aiConfig.topP(),
          maxRetries: 2,
          providerOptions: aiConfig.providerOptions(),
          tools: aiSdkTools,
          activeTools,
          // biome-ignore lint/style/useNamingConvention: third-party controlled
          experimental_repairToolCall:
            toolCallRepair<CompleteTools>(modelManager),
          abortSignal,
          onAbort: ({ steps }) => {
            logger.debug(`Aborting and processing ${steps.length} steps`);
            steps.forEach((step) => {
              sessionManager.appendResponseMessages(step.response.messages);
            });
          },
          onError({ error }) {
            if (
              typeof error === "object" &&
              error != null &&
              "message" in error
            ) {
              logger.error(error.message);
            } else {
              logger.error(error);
            }
          },
        });

        let accumulatedText = "";
        let accumulatedReasoning = "";

        const thisStepToolCalls: { toolName: string }[] = [];
        const thisStepToolResults: { toolName: string }[] = [];
        this._state.steps.push({
          toolCalls: thisStepToolCalls,
          toolResults: thisStepToolResults,
        });

        const pendingToolCalls: PendingToolCall[] = [];

        for await (const chunk of result.stream) {
          if (firstTokenAt === null && isContentChunk(chunk)) {
            firstTokenAt = performance.now();
            logger.info(
              {
                event: "model.first_token",
                requestId,
                ttftMs: Math.round(firstTokenAt - requestStart),
              },
              "model.first_token",
            );
          }
          const event = this.handleStreamChunk(chunk, {
            accumulatedText,
            accumulatedReasoning,
            toolsCalled,
            tools,
            pendingToolCalls,
          });
          if (event) {
            accumulatedText = event.accumulatedText;
            accumulatedReasoning = event.accumulatedReasoning;
            yield event.agentEvent;
          }
        }

        const modelStreamEnd = performance.now();

        // ============================================================
        // PARALLEL TOOL EXECUTION
        // ============================================================

        const toolStart = performance.now();
        const { collectedEvents, parallelToolMessages } =
          await this.executeToolsInParallel({
            pendingToolCalls,
            abortSignal,
            toolsCalled,
            stepToolCalls: thisStepToolCalls,
            stepToolResults: thisStepToolResults,
          });
        const toolEnd = performance.now();

        // Yield all collected events in order
        for (const event of collectedEvents) {
          yield event;
        }

        // Get response and tool calls
        const response = await result.response;
        const responseMessages = response.messages;

        // Filter out tool-result messages from responseMessages since we manually
        // execute tools and add their results via appendToolMessages below.
        // The AI SDK may include tool-result messages in response.messages even
        // when tools don't have an execute function.
        const nonToolMessages = responseMessages.filter(
          (msg) => msg.role !== "tool",
        );
        sessionManager.appendResponseMessages(nonToolMessages);

        const stepUsage = await result.usage;
        this.updateUsageStats(stepUsage, sessionManager);

        // If finishReason is not tool-calls, break
        const finishReason = await result.finishReason;

        const modelResponseMs = modelStreamEnd - requestStart;
        const toolMs = toolEnd - toolStart;
        const wallClockMs = performance.now() - requestStart;
        const outputTokens = stepUsage.outputTokens ?? 0;
        logger.info(
          {
            event: "model.request.end",
            requestId,
            model: modelConfig.id,
            provider: modelConfig.provider,
            modelResponseMs: Math.round(modelResponseMs),
            ttftMs:
              firstTokenAt === null
                ? null
                : Math.round(firstTokenAt - requestStart),
            inputTokens: stepUsage.inputTokens ?? 0,
            outputTokens,
            outputTokensPerSecond:
              modelResponseMs > 0
                ? Number((outputTokens / (modelResponseMs / 1000)).toFixed(2))
                : 0,
            reasoningTokens: stepUsage.outputTokenDetails.reasoningTokens ?? 0,
            finishReason,
            retryCount: consecutiveErrors,
            providerRequestId: response.id ?? null,
            providerHeaders: extractProviderHeaders(response.headers),
          },
          "model.request.end",
        );
        sessionManager.recordTurnTiming({
          wallClockMs,
          modelMs: modelResponseMs,
          toolMs,
        });

        if (finishReason !== "tool-calls") {
          yield {
            type: "agent-stop",
          };
          hasEmittedTerminalEvent = true;
          break;
        }

        sessionManager.appendToolMessages(parallelToolMessages);

        // Consume the rest of the team if necessary
        // await result.consumeStream();

        yield {
          type: "step-stop",
        };

        // continue iterations
        consecutiveErrors = 0;
        iter += 1;
      } catch (error) {
        consecutiveErrors += 1;

        // Handle AI SDK invalid tool input errors gracefully
        if (InvalidToolInputError.isInstance(error)) {
          logger.warn(
            error,
            `Invalid tool input detected - returning error to allow recovery (attempt ${consecutiveErrors}/${maxRetries + 1})`,
          );
          yield {
            type: "agent-error",
            message: `Tool input validation failed: ${(error as Error).message}. Try again with valid arguments.`,
          };
          // Continue loop to allow user to provide corrected input
        } else {
          // Extract additional error context for better debugging
          const err = error as Error & {
            cause?: unknown;
            response?: { status: number; statusText: string };
          };
          const errorContext: Record<string, unknown> = {
            modelId: modelConfig?.id ?? langModel.modelId ?? "unknown",
            sessionId: sessionManager.getSessionId(),
            messageCount: sessionManager.get().length,
            attempt: consecutiveErrors,
            maxRetries,
          };

          // Add response info if available (common for Bad Request errors from AI SDK)
          if (err.cause) {
            const cause = err.cause as {
              response?: { status: number; statusText: string; body?: unknown };
            };
            if (cause.response) {
              errorContext["responseStatus"] = cause.response.status;
              errorContext["responseStatusText"] = cause.response.statusText;
              // Add body snippet if available (truncated to avoid log bloat)
              if (
                cause.response.body &&
                typeof cause.response.body === "object"
              ) {
                const body = cause.response.body as Record<string, unknown>;
                errorContext["responseBody"] = JSON.stringify(body).slice(
                  0,
                  500,
                );
              }
            }
          }

          logger.error(
            { ...errorContext, error: err },
            `Error on manual agent loop streamText (attempt ${consecutiveErrors}/${maxRetries + 1})`,
          );

          const errorMsg =
            (error as Error).message.length > 100
              ? `${(error as Error).message.slice(0, 100)}...`
              : (error as Error).message;

          yield {
            type: "agent-error",
            message: errorMsg,
          };

          if (NoOutputGeneratedError.isInstance(error)) {
            hasEmittedTerminalEvent = true;
            break;
          }
        }

        // Break loop if we exceed max retries
        if (consecutiveErrors > maxRetries) {
          yield {
            type: "agent-error",
            message: `Exceeded maximum retry attempts (${maxRetries}). Stopping manual loop.`,
          };
          hasEmittedTerminalEvent = true;
          break;
        }
      } finally {
        this._state.timestamps.stop = performance.now();
      }
    }
    // Emit agent-stop if loop ended without emitting a terminal event (maxIterations reached)
    if (!hasEmittedTerminalEvent) {
      yield {
        type: "agent-stop",
      };
    }
  }

  abort() {
    this.abortController.abort();
    // Reset the abort controller for the next run
    this.abortController = new AbortController();
  }

  resetState() {
    const {
      modelManager,
      // sessionManager,
    } = this.opts;

    this._state = {
      modelId: modelManager.getModel("repl").modelId,
      modelConfig: modelManager.getModelMetadata("repl"),
      usage: {
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        reasoningTokens: 0,
        cachedInputTokens: 0,
        inputTokenDetails: {
          noCacheTokens: 0,
          cacheReadTokens: 0,
          cacheWriteTokens: 0,
        },
        outputTokenDetails: {
          textTokens: 0,
          reasoningTokens: 0,
        },
      },
      totalUsage: {
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        reasoningTokens: 0,
        cachedInputTokens: 0,
        inputTokenDetails: {
          noCacheTokens: 0,
          cacheReadTokens: 0,
          cacheWriteTokens: 0,
        },
        outputTokenDetails: {
          textTokens: 0,
          reasoningTokens: 0,
        },
      },
      steps: [],
      timestamps: {
        start: 0,
        stop: 0,
      },
    };

    return this._state;
  }

  private handleStreamChunk(
    // biome-ignore lint/suspicious/noExplicitAny: AI SDK stream chunks are a complex discriminated union
    chunk: any,
    ctx: {
      accumulatedText: string;
      accumulatedReasoning: string;
      toolsCalled: Map<string, ToolEvent[]>;
      tools: CompleteToolSet;
      pendingToolCalls: PendingToolCall[];
    },
  ): {
    agentEvent: AgentEvent;
    accumulatedText: string;
    accumulatedReasoning: string;
  } | null {
    if (chunk.type === "reasoning-start") {
      return {
        agentEvent: { type: "thinking-start", content: "" },
        accumulatedText: ctx.accumulatedText,
        accumulatedReasoning: ctx.accumulatedReasoning,
      };
    }
    if (chunk.type === "reasoning-delta") {
      const accumulatedReasoning = ctx.accumulatedReasoning + chunk.text;
      return {
        agentEvent: { type: "thinking", content: accumulatedReasoning },
        accumulatedText: ctx.accumulatedText,
        accumulatedReasoning,
      };
    }
    if (chunk.type === "reasoning-end") {
      return {
        agentEvent: {
          type: "thinking-end",
          content: ctx.accumulatedReasoning,
        },
        accumulatedText: ctx.accumulatedText,
        accumulatedReasoning: ctx.accumulatedReasoning,
      };
    }
    if (chunk.type === "text-start") {
      return {
        agentEvent: { type: "message-start", role: "assistant", content: "" },
        accumulatedText: ctx.accumulatedText,
        accumulatedReasoning: ctx.accumulatedReasoning,
      };
    }
    if (chunk.type === "text-delta") {
      const accumulatedText = ctx.accumulatedText + chunk.text;
      return {
        agentEvent: {
          type: "message",
          role: "assistant",
          content: accumulatedText,
        },
        accumulatedText,
        accumulatedReasoning: ctx.accumulatedReasoning,
      };
    }
    if (chunk.type === "text-end") {
      return {
        agentEvent: {
          type: "message-end",
          role: "assistant",
          content: ctx.accumulatedText,
        },
        accumulatedText: ctx.accumulatedText,
        accumulatedReasoning: ctx.accumulatedReasoning,
      };
    }
    if (chunk.type === "tool-call") {
      const call = chunk;
      const toolName = call.toolName as keyof CompleteToolSet;
      const iTool = ctx.tools[toolName];

      const agentEvent = this.processToolEvent(ctx.toolsCalled, {
        type: "tool-call-start",
        name: toolName,
        toolCallId: call.toolCallId,
        // biome-ignore lint/suspicious/noExplicitAny: unknown
        msg: iTool ? iTool.display(call.input as any) : "",
        args: call.input,
      });

      ctx.pendingToolCalls.push({ call, toolName, iTool });

      return {
        agentEvent,
        accumulatedText: ctx.accumulatedText,
        accumulatedReasoning: ctx.accumulatedReasoning,
      };
    }
    return null;
  }

  private updateUsageStats(
    stepUsage: LanguageModelUsage,
    sessionManager: SessionManager,
  ): void {
    // Extract values with null coalescing once
    const inputTokens = stepUsage.inputTokens ?? 0;
    const outputTokens = stepUsage.outputTokens ?? 0;
    const totalTokens = stepUsage.totalTokens ?? 0;
    const cacheReadTokens = stepUsage.inputTokenDetails.cacheReadTokens ?? 0;
    const reasoningTokens = stepUsage.outputTokenDetails.reasoningTokens ?? 0;

    // Update step usage
    this._state.usage.inputTokens = inputTokens;
    this._state.usage.outputTokens = outputTokens;
    this._state.usage.totalTokens = totalTokens;
    this._state.usage.inputTokenDetails.cacheReadTokens = cacheReadTokens;
    this._state.usage.cachedInputTokens = cacheReadTokens;
    this._state.usage.outputTokenDetails.reasoningTokens = reasoningTokens;
    this._state.usage.reasoningTokens = reasoningTokens;
    sessionManager.setContextWindow(totalTokens);

    // Update total usage (accumulate)
    this._state.totalUsage.inputTokens += inputTokens;
    this._state.totalUsage.outputTokens += outputTokens;
    this._state.totalUsage.totalTokens += totalTokens;
    this._state.totalUsage.inputTokenDetails.cacheReadTokens += cacheReadTokens;
    this._state.totalUsage.cachedInputTokens += cacheReadTokens;
    this._state.totalUsage.outputTokenDetails.reasoningTokens +=
      reasoningTokens;
    this._state.totalUsage.reasoningTokens += reasoningTokens;

    this.opts.tokenTracker.trackUsage("repl", stepUsage);

    sessionManager.recordTurnUsage({
      inputTokens,
      outputTokens,
      totalTokens,
      cachedInputTokens: cacheReadTokens,
      reasoningTokens,
      inputTokenDetails: {
        noCacheTokens: stepUsage.inputTokenDetails.noCacheTokens ?? 0,
        cacheReadTokens,
        cacheWriteTokens: stepUsage.inputTokenDetails.cacheWriteTokens ?? 0,
      },
      outputTokenDetails: {
        textTokens: stepUsage.outputTokenDetails.textTokens ?? 0,
        reasoningTokens,
      },
    });
  }

  private processToolEvent(
    toolsCalled: Map<string, ToolEvent[]>,
    event: ToolEvent,
  ): ToolCallLifeCycle {
    const toolCallId = event.toolCallId;
    let events: ToolEvent[];
    if (toolsCalled.has(toolCallId)) {
      const currentEvents = toolsCalled.get(toolCallId);
      if (currentEvents) {
        events = currentEvents;
        events.push(event);
      } else {
        events = [event];
        toolsCalled.set(toolCallId, events);
      }
    } else {
      events = [event];
      toolsCalled.set(toolCallId, events);
    }
    return {
      type: "tool-call-lifecycle",
      toolCallId: toolCallId,
      events,
    };
  }

  private async executeToolsInParallel({
    pendingToolCalls,
    abortSignal,
    toolsCalled,
    stepToolCalls,
    stepToolResults,
  }: {
    pendingToolCalls: PendingToolCall[];
    abortSignal: AbortSignal | undefined;
    toolsCalled: Map<string, ToolEvent[]>;
    stepToolCalls: Array<{ toolName: string }>;
    stepToolResults: Array<{ toolName: string }>;
  }): Promise<{
    collectedEvents: ToolCallLifeCycle[];
    parallelToolMessages: ToolModelMessage[];
  }> {
    const results: Array<{
      toolCallId: string;
      toolName: string;
      resultOutput: string;
      success: boolean;
    }> = new Array(pendingToolCalls.length);

    const collectedEvents: ToolCallLifeCycle[] = [];

    // Phase 2a: Validate all inputs first (before any execution)
    const validationErrors = this.validateToolInputs(pendingToolCalls);

    // Emit validation errors and mark those tools as failed
    for (const { index, error } of validationErrors) {
      const { call, toolName } = pendingToolCalls[index];
      const event = this.processToolEvent(toolsCalled, {
        type: "tool-call-error",
        name: toolName,
        toolCallId: call.toolCallId,
        msg: error,
        args: null,
      });
      collectedEvents.push(event);
      results[index] = {
        toolCallId: call.toolCallId,
        toolName,
        resultOutput: error,
        success: false,
      };
    }

    // Phase 2b: Execute valid tools in parallel
    const sessionManager = this.opts.sessionManager;
    const executionPromises = pendingToolCalls.map(async (pending, index) => {
      if (validationErrors.some((e) => e.index === index)) {
        return;
      }

      await this.executeSingleTool({
        pending,
        index,
        sessionManager,
        abortSignal,
        toolsCalled,
        collectedEvents,
        results,
        stepToolCalls,
        stepToolResults,
      });
    });

    // Wait for all executions to complete
    await Promise.allSettled(executionPromises);

    // Phase 2c: Build toolMessages in original call order
    const parallelToolMessages = this.buildToolMessages(results);

    return { collectedEvents, parallelToolMessages };
  }

  private validateToolInputs(
    pendingToolCalls: PendingToolCall[],
  ): Array<{ index: number; error: string }> {
    const validationErrors: Array<{ index: number; error: string }> = [];

    for (let i = 0; i < pendingToolCalls.length; i++) {
      const { call } = pendingToolCalls[i];

      // Check for invalid call from AI SDK
      if (call.invalid) {
        validationErrors.push({
          index: i,
          error: String(call.error),
        });
        continue;
      }

      // Validate JSON input
      if (typeof call.input === "string") {
        try {
          JSON.parse(call.input);
        } catch {
          validationErrors.push({
            index: i,
            error: `Invalid tool input: malformed JSON. Received: "${call.input.slice(0, 50)}${call.input.length > 50 ? "..." : ""}". Expected a JSON object.`,
          });
        }
      } else if (call.input === null || call.input === undefined) {
        validationErrors.push({
          index: i,
          error:
            "Invalid tool input: received null/undefined. Expected a JSON object matching the schema.",
        });
      }
    }

    return validationErrors;
  }

  private async executeSingleTool({
    pending,
    index,
    sessionManager,
    abortSignal,
    toolsCalled,
    collectedEvents,
    results,
    stepToolCalls,
    stepToolResults,
  }: {
    pending: PendingToolCall;
    index: number;
    sessionManager: SessionManager;
    abortSignal: AbortSignal | undefined;
    toolsCalled: Map<string, ToolEvent[]>;
    collectedEvents: ToolCallLifeCycle[];
    results: Array<{
      toolCallId: string;
      toolName: string;
      resultOutput: string;
      success: boolean;
    }>;
    stepToolCalls: Array<{ toolName: string }>;
    stepToolResults: Array<{ toolName: string }>;
  }): Promise<void> {
    const { call, toolName, iTool } = pending;

    // Track in step stats
    stepToolCalls.push({ toolName });

    if (!iTool) {
      const errorMsg = `No executor for tool ${toolName}`;
      const event = this.processToolEvent(toolsCalled, {
        type: "tool-call-error",
        name: toolName,
        toolCallId: call.toolCallId,
        msg: errorMsg,
        args: null,
      });
      collectedEvents.push(event);
      results[index] = {
        toolCallId: call.toolCallId,
        toolName,
        resultOutput: errorMsg,
        success: false,
      };
      return;
    }

    const toolExec = iTool.execute as ToolExecuteFunction<
      unknown,
      string,
      unknown
    >;

    try {
      const output = await toolExec(call.input, {
        toolCallId: call.toolCallId,
        context: undefined,
        messages: sessionManager.get(),
        abortSignal,
      });
      const resultOutput = formatToolResult(output);

      const event = this.processToolEvent(toolsCalled, {
        type: "tool-call-end",
        name: call.toolName,
        toolCallId: call.toolCallId,
        msg: resultOutput,
        args: call.input,
      });
      collectedEvents.push(event);

      stepToolResults.push({ toolName });

      results[index] = {
        toolCallId: call.toolCallId,
        toolName,
        resultOutput,
        success: true,
      };
    } catch (err) {
      const resultOutput = `Tool error: ${
        err instanceof Error ? err.message : String(err)
      }`;

      const event = this.processToolEvent(toolsCalled, {
        type: "tool-call-error",
        name: toolName,
        toolCallId: call.toolCallId,
        msg: resultOutput,
        args: null,
      });
      collectedEvents.push(event);

      results[index] = {
        toolCallId: call.toolCallId,
        toolName,
        resultOutput,
        success: false,
      };
    }
  }

  private buildToolMessages(
    results: Array<{
      toolCallId: string;
      toolName: string;
      resultOutput: string;
      success: boolean;
    }>,
  ): ToolModelMessage[] {
    const parallelToolMessages: ToolModelMessage[] = [];
    for (const result of results) {
      if (!result) continue; // Skip if somehow undefined

      parallelToolMessages.push({
        role: "tool",
        content: [
          {
            type: "tool-result",
            toolName: result.toolName,
            toolCallId: result.toolCallId,
            output: {
              type: "text",
              value: result.resultOutput,
            },
          },
        ],
      } as const);
    }
    return parallelToolMessages;
  }
}

function formatToolResult(value: unknown): string {
  if (typeof value === "string") {
    return value;
  }

  if (value === undefined) {
    return "";
  }

  if (value === null) {
    return "null";
  }

  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }

  try {
    const serialized = JSON.stringify(value);
    if (typeof serialized === "string") {
      return serialized;
    }
  } catch {
    // noop - fallback below
  }

  return String(value);
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

    logger.error(
      `Attemping to repair tool call: ${toolCall.toolName} - ${toolCall.input}`,
    );

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
