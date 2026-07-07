import { randomUUID } from "node:crypto";
import EventEmitter from "node:events";
import {
  mkdir,
  readdir,
  readFile,
  rename,
  stat,
  unlink,
  writeFile,
} from "node:fs/promises";
import { basename, join } from "node:path";
import { isString } from "@travisennis/stdlib/typeguards";
import {
  type AssistantModelMessage,
  type FilePart,
  generateText,
  type ImagePart, // Added ImagePart
  type ModelMessage,
  type TextPart,
  type ToolModelMessage,
  type UserModelMessage,
} from "ai";
import type { ModelManager } from "../models/manager.ts";
import type { TokenTracker } from "../tokens/tracker.ts";
import { dedent } from "../utils/dedent.ts";
import { logger } from "../utils/logger.ts";

// Define a type for the items that can be passed in the first argument
export type UserMessageContentItem = string | FilePart | ImagePart;

export function createUserMessage(
  contentItems: UserMessageContentItem[],
  prompt?: string,
): UserModelMessage {
  const messageParts: (FilePart | ImagePart | TextPart)[] = [];

  // Process content items (images and pre-defined texts)
  for (const item of contentItems) {
    if (typeof item === "string") {
      if (item.trim().length > 0) {
        messageParts.push({ type: "text", text: item });
      }
    } else if (item.type === "file" || item.type === "image") {
      messageParts.push(item);
    }
  }

  // Add the main prompt text if provided
  if (prompt && prompt.trim().length > 0) {
    messageParts.push({ type: "text", text: prompt });
  }

  return {
    role: "user",
    content: messageParts,
  };
}

function createAssistantMessage(content: string): AssistantModelMessage {
  return {
    role: "assistant",
    content: [
      {
        type: "text",
        text: content,
      },
    ],
  };
}

function isPlainObject(x: unknown): x is Record<string, unknown> {
  return typeof x === "object" && x !== null && !Array.isArray(x);
}

function sanitizeToolCallInput(input: unknown): {
  sanitized: Record<string, unknown>;
  isValid: boolean;
} {
  // If it's already a valid plain object, return as-is
  if (isPlainObject(input)) {
    return { sanitized: input, isValid: true };
  }

  // If it's a string, try to parse it as JSON
  if (typeof input === "string") {
    try {
      const parsed = JSON.parse(input);
      if (isPlainObject(parsed)) {
        return { sanitized: parsed, isValid: true };
      }
    } catch {
      // JSON parsing failed - this is malformed JSON
      logger.warn(
        { originalInput: input.slice(0, 100) },
        "Filtered tool call with malformed JSON input",
      );
      return { sanitized: {}, isValid: false };
    }
  }

  // For any other case (undefined, null, array, etc.)
  logger.warn(
    { originalInput: typeof input === "string" ? input.slice(0, 100) : input },
    "Filtered tool call with invalid input type",
  );
  return { sanitized: {}, isValid: false };
}

/**
 * Check if a tool call input is valid and return sanitized version.
 * This helper reduces complexity by extracting validation logic.
 */
function sanitizeToolCallPart(
  part: Extract<AssistantModelMessage["content"], readonly unknown[]>[number],
): typeof part | null {
  if (part.type !== "tool-call") {
    return null; // Not a tool call, keep as-is
  }

  const { sanitized, isValid } = sanitizeToolCallInput(part.input);
  if (!isValid) {
    logger.debug(
      { toolName: part.toolName },
      "Sanitized invalid tool call input",
    );
  }

  // Always keep tool calls so the model can see the corresponding tool-result
  return { ...part, input: sanitized };
}

export function sanitizeResponseMessages(
  messages: ResponseMessage[],
): ResponseMessage[] {
  return messages.map((msg) => {
    if (msg.role !== "assistant" || !Array.isArray(msg.content)) {
      return msg;
    }

    // Sanitize tool call inputs while always preserving the tool-call itself.
    // The model needs to see every tool-call to match it with its tool-result.
    const sanitizedContent: typeof msg.content = [];

    for (const part of msg.content) {
      const sanitized = sanitizeToolCallPart(part);
      if (sanitized === null) {
        // Not a tool call, keep as-is
        sanitizedContent.push(part);
      } else {
        sanitizedContent.push(sanitized);
      }
    }

    return { ...msg, content: sanitizedContent } as AssistantModelMessage;
  });
}

/**
A message that was generated during the generation process.
It can be either an assistant message or a tool message.
 */
type ResponseMessage = AssistantModelMessage | ToolModelMessage;

/**
 * Compact token usage stored in session files.
 * Only stores total aggregated usage and last turn's usage
 * to minimize session file size while maintaining necessary information.
 */
type SessionTokenUsage = {
  /** Aggregated total for entire session */
  total: {
    inputTokens: number;
    outputTokens: number;
    totalTokens: number;
    cachedInputTokens: number;
    reasoningTokens: number;
    estimatedCost: number;
  };
  /** Last turn's usage (for context window display) */
  lastTurn: {
    inputTokens: number;
    outputTokens: number;
    totalTokens: number;
    cachedInputTokens: number;
    reasoningTokens: number;
    estimatedCost: number;
  };
};

/**
 * Per-turn timing accumulated across a session.
 * Mirrors the SessionTokenUsage shape (total plus lastTurn) so summaries can
 * separate model response time from tool execution time.
 */
type SessionTiming = {
  /** Aggregated totals for the entire session */
  total: {
    wallClockMs: number;
    modelMs: number;
    toolMs: number;
    turns: number;
  };
  /** Last turn's timing */
  lastTurn: {
    wallClockMs: number;
    modelMs: number;
    toolMs: number;
  };
};

/**
 * @deprecated Use SessionTokenUsage instead. Kept for backward compatibility
 * with older session files that may have the old format.
 */
type TokenUsageTurn = {
  stepIndex: number;
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
  timestamp: number;
  estimatedCost: number;
};

type SavedMessageHistory = {
  project: string;
  sessionId: string;
  modelId: string;
  title: string;
  createdAt: Date;
  updatedAt: Date;
  messages: ModelMessage[];
  tokenUsage?: SessionTokenUsage | TokenUsageTurn[];
  timing?: SessionTiming;
  metadata?: Record<string, unknown>;
};

type RawMessageHistory = Omit<
  SavedMessageHistory,
  "createdAt" | "updatedAt"
> & {
  createdAt?: string;
  updatedAt?: string;
};

interface MessageHistoryEvents {
  "update-title": [string];
  "clear-history": [];
}

export class SessionManager extends EventEmitter<MessageHistoryEvents> {
  private history: ModelMessage[];
  private sessionId: string;
  private modelId: string;
  private title: string;
  private createdAt: Date;
  private updatedAt: Date;
  private stateDir: string;
  private contextWindow: number;
  private modelManager: ModelManager;
  private tokenTracker: TokenTracker;
  private tokenUsage: SessionTokenUsage | null;
  private timing: SessionTiming | null;
  private transientMessages: UserModelMessage[] = [];
  private metadata: Record<string, unknown> = {};

  constructor({
    stateDir,
    modelManager,
    tokenTracker,
  }: {
    stateDir: string;
    modelManager: ModelManager;
    tokenTracker: TokenTracker;
  }) {
    super();
    this.history = [];
    this.sessionId = randomUUID();
    this.modelId = modelManager.getModel("repl").modelId;
    this.title = "";
    this.createdAt = new Date();
    this.updatedAt = new Date();
    this.stateDir = stateDir;
    this.contextWindow = 0;
    this.modelManager = modelManager;
    this.tokenTracker = tokenTracker;
    this.tokenUsage = null;
    this.timing = null;
  }

  private createEmptyTokenUsage(): SessionTokenUsage {
    return {
      total: {
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        cachedInputTokens: 0,
        reasoningTokens: 0,
        estimatedCost: 0,
      },
      lastTurn: {
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        cachedInputTokens: 0,
        reasoningTokens: 0,
        estimatedCost: 0,
      },
    };
  }

  private createEmptyTiming(): SessionTiming {
    return {
      total: {
        wallClockMs: 0,
        modelMs: 0,
        toolMs: 0,
        turns: 0,
      },
      lastTurn: {
        wallClockMs: 0,
        modelMs: 0,
        toolMs: 0,
      },
    };
  }

  create(modelId: string) {
    this.clear();
    this.modelId = modelId;
    this.sessionId = randomUUID();
    this.title = "";
    this.createdAt = new Date();
    this.updatedAt = new Date();
  }

  setModelId(modelId: string) {
    this.modelId = modelId;
  }

  private validMessage(msg: ModelMessage) {
    // Filter out messages with empty content arrays
    if (Array.isArray(msg.content) && msg.content.length === 0) {
      return false;
    }

    // Filter out assistant messages with empty text fields
    if (
      msg.role === "assistant" &&
      Array.isArray(msg.content) &&
      msg.content.length === 1 &&
      msg.content[0]?.type === "text" &&
      msg.content[0]?.text === ""
    ) {
      return false;
    }

    return true;
  }

  get() {
    const history = [...this.history].filter(this.validMessage);
    if (this.transientMessages.length > 0 && history.length > 0) {
      const lastIndex = history.length - 1;
      const last = history[lastIndex] as ModelMessage;

      // Merge transient user messages into the last history entry when both are
      // user role, avoiding consecutive user messages that some providers reject.
      if (last.role === "user") {
        const lastContent = Array.isArray(last.content) ? last.content : [];
        const transientContent = this.transientMessages.flatMap((m) =>
          Array.isArray(m.content) ? m.content : [],
        );
        return [
          ...history.slice(0, lastIndex),
          {
            ...last,
            content: [...lastContent, ...transientContent],
          } as ModelMessage,
        ];
      }

      return [...history.slice(0, lastIndex), ...this.transientMessages, last];
    }
    return history;
  }

  setTransientMessages(messages: UserModelMessage[]): void {
    this.transientMessages = messages;
  }

  clearTransientMessages(): void {
    this.transientMessages = [];
  }

  setMetadata(key: string, value: unknown): void {
    this.metadata[key] = value;
  }

  getMetadata(key: string): unknown {
    return this.metadata[key];
  }

  clear() {
    this.history.length = 0;
    this.transientMessages = [];
    this.metadata = {};
    this.contextWindow = 0;
    this.tokenUsage = null;
    this.timing = null;
    this.emit("clear-history");
  }

  setContextWindow(contextWindow: number) {
    if (contextWindow < 0) {
      throw new Error("Context window cannot be negative");
    }
    this.contextWindow = contextWindow;
  }

  getContextWindow() {
    return this.contextWindow;
  }

  appendUserMessage(msg: string): void;
  appendUserMessage(msg: UserModelMessage): void;
  appendUserMessage(msg: string | UserModelMessage) {
    const now = new Date();
    const msgObj = isString(msg) ? createUserMessage([], msg) : msg;
    if (
      this.history.length === 0 &&
      msgObj.content &&
      msgObj.content.length > 0
    ) {
      const textPart = msgObj.content.at(-1) as TextPart;
      if (textPart?.text && textPart.text.trim() !== "") {
        void this.generateTitle(textPart.text);
      }
      this.createdAt = now;
    }
    this.updatedAt = now;
    this.history.push(msgObj);
  }

  appendAssistantMessage(msg: string): void;
  appendAssistantMessage(msg: AssistantModelMessage): void;
  appendAssistantMessage(msg: string | AssistantModelMessage) {
    this.updatedAt = new Date();
    const msgObj = isString(msg) ? createAssistantMessage(msg) : msg;
    this.history.push(msgObj);
  }

  appendToolMessages(toolResultMessages: ToolModelMessage[]) {
    this.updatedAt = new Date();

    // Collect existing tool-result IDs to prevent duplicates
    // (the AI SDK may include tool-results that acai also creates)
    const existingIds = new Set<string>();
    for (const msg of this.history) {
      if (msg.role === "tool" && Array.isArray(msg.content)) {
        for (const part of msg.content) {
          if (part.type === "tool-result") {
            existingIds.add(part.toolCallId);
          }
        }
      }
    }

    const deduplicated = toolResultMessages.filter((msg) => {
      if (!Array.isArray(msg.content)) return true;
      return !msg.content.some(
        (part) =>
          part.type === "tool-result" && existingIds.has(part.toolCallId),
      );
    });

    this.history.push(...deduplicated);
  }

  appendResponseMessages(responseMessages: ResponseMessage[]) {
    this.updatedAt = new Date();
    // Sanitize tool call inputs to prevent malformed JSON from poisoning history
    const sanitizedMessages = sanitizeResponseMessages(responseMessages);
    // Filter out messages with empty content arrays
    const validMessages = sanitizedMessages.filter(this.validMessage);
    this.history.push(...validMessages);
  }

  isEmpty() {
    return this.history.length === 0;
  }

  async save() {
    const msgHistoryDir = this.stateDir;
    const fileName = this.getSessionFileName();
    const filePath = join(msgHistoryDir, fileName);
    const tempFilePath = `${filePath}.tmp`;

    // Validate data before writing
    if (!this.sessionId || this.sessionId.trim() === "") {
      throw new Error("Cannot save: sessionId is empty");
    }

    if (!Array.isArray(this.history)) {
      throw new Error("Cannot save: history is not an array");
    }

    const project = basename(process.cwd());

    const output: SavedMessageHistory = {
      project,
      sessionId: this.sessionId,
      modelId: this.modelId,
      title: this.title,
      createdAt: this.createdAt,
      updatedAt: this.updatedAt,
      messages: this.history,
      ...(this.tokenUsage !== null ? { tokenUsage: this.tokenUsage } : {}),
      ...(this.timing !== null ? { timing: this.timing } : {}),
      metadata:
        Object.keys(this.metadata).length > 0 ? this.metadata : undefined,
    };

    try {
      // Ensure directory exists
      await mkdir(msgHistoryDir, { recursive: true });

      // Write to temporary file first
      await writeFile(tempFilePath, JSON.stringify(output, null, 2));

      // Atomically rename to final file
      await rename(tempFilePath, filePath);

      logger.info(`Message history saved to ${filePath}`);
    } catch (error) {
      // Clean up temp file if it exists
      try {
        await unlink(tempFilePath);
      } catch (_cleanupError) {
        // Ignore cleanup errors
      }

      // Check if it's an ENOENT error from rename (temp file doesn't exist)
      if (
        error instanceof Error &&
        "code" in error &&
        error.code === "ENOENT"
      ) {
        logger.warn(
          `Temp file missing during save for ${filePath}, write may have been interrupted`,
        );
      } else {
        logger.error(error, `Failed to save message history to ${filePath}:`);
        // Don't throw - just log. This is called from interrupt handlers
        // and we don't want to crash the program on save failure.
      }
    }
  }

  private async generateTitle(message: string) {
    // Skip title generation if message is empty
    if (!message || message.trim() === "") {
      return;
    }

    const app = "title-conversation";

    const systemPrompt = dedent`
You are an assistant who is tasked to analyze messages to generate a conversation topic that can be used as a conversation title. For each message, generate a short title that captures the topic. Return only the title with no other text.

## Examples:

<example>
Message:
How do I implement authentication in my Express app?

Title:
Express Authentication Implementation
</example>

<example>
Message:
Can you help me debug this React component that isn't rendering correctly?

Title:
React Component Rendering Debug";
</example>
`;

    let title = "";

    try {
      const model = this.modelManager.getModel(app);

      const result = await generateText({
        model,
        instructions: systemPrompt,
        prompt: `Message:\n${message}\n\nTitle:`,
      });

      this.tokenTracker.trackUsage(app, result.usage);

      if (result.text && result.text.length > 0) {
        title = result.text;
        this.title = title;
      }
    } catch (error) {
      logger.error(
        error,
        "[generateTitle] Failed to generate conversation title:",
      );
    }

    if (title.length === 0) {
      this.title =
        message.slice(0, 50).trim() + (message.length > 50 ? "..." : "");
    }
    this.emit("update-title", this.title);
  }

  getFirstUserMessage(): UserModelMessage | undefined {
    const firstUser = this.get().find(
      (msg): msg is UserModelMessage => msg.role === "user",
    );
    return firstUser;
  }

  getLastUserMessage(): UserModelMessage | undefined {
    const userMsg = this.history.findLast(
      (value): value is UserModelMessage => value.role === "user",
    );
    return userMsg;
  }

  /**
   * Extracts the last message from the conversation history for display purposes.
   * Prioritizes assistant messages, falls back to user messages if no assistant messages exist.
   */
  getLastMessage(): string | null {
    const messages = this.get();
    if (messages.length === 0) {
      return null;
    }

    // Find the last assistant message, or fall back to last user message
    const reversedMessages = [...messages].reverse();
    const lastAssistant = reversedMessages.find(
      (msg): msg is AssistantModelMessage => msg.role === "assistant",
    );
    const lastUser = reversedMessages.find(
      (msg): msg is UserModelMessage => msg.role === "user",
    );

    const targetMessage = lastAssistant || lastUser;
    if (!targetMessage) {
      return null;
    }

    // Extract text content from the message
    if (Array.isArray(targetMessage.content)) {
      const textParts = targetMessage.content.filter(
        (part): part is TextPart =>
          part.type === "text" && part.text.trim().length > 0,
      );
      if (textParts.length === 0) {
        return null;
      }
      return textParts.map((part) => part.text).join("\n");
    }

    // Handle string content (though this should be rare with current implementation)
    if (
      typeof targetMessage.content === "string" &&
      targetMessage.content.trim().length > 0
    ) {
      return targetMessage.content;
    }

    return null;
  }

  // Getter methods for session information
  getSessionId(): string {
    return this.sessionId;
  }

  getModelId(): string {
    return this.modelId;
  }

  getTitle(): string {
    return this.title;
  }

  getCreatedAt(): Date {
    return this.createdAt;
  }

  getUpdatedAt(): Date {
    return this.updatedAt;
  }

  getSessionFileName(): string {
    return `session-${this.sessionId}.json`;
  }

  getSessionFilePath(): string {
    return join(this.stateDir, this.getSessionFileName());
  }

  static async load(
    stateDir: string,
    count = 10, // Add count parameter with default
  ): Promise<SavedMessageHistory[]> {
    try {
      const files = await readdir(stateDir);
      const sessionFiles = files.filter(
        (file) => file.startsWith("session-") && file.endsWith(".json"),
      );

      // Get file stats and sort by modification time (newest first)
      const fileStatsPromises = sessionFiles.map(async (fileName) => {
        const filePath = join(stateDir, fileName);
        try {
          const fileStat = await stat(filePath);
          return {
            fileName,
            filePath,
            modifiedTime: fileStat.mtime,
          };
        } catch (error) {
          console.error(`Error getting stats for file ${filePath}:`, error);
          return null;
        }
      });

      const fileStats = await Promise.all(fileStatsPromises);

      // Filter out null results and sort by modification time (newest first)
      const sortedFiles = fileStats
        .filter(
          (
            stat,
          ): stat is {
            fileName: string;
            filePath: string;
            modifiedTime: Date;
          } => stat !== null,
        )
        .sort((a, b) => b.modifiedTime.getTime() - a.modifiedTime.getTime())
        .slice(0, count)
        .map((stat) => stat.fileName);

      const fileReadPromises = sortedFiles.map(async (fileName) => {
        const filePath = join(stateDir, fileName);
        try {
          // Check file stats first to avoid reading empty files
          const stats = await stat(filePath);
          if (stats.size === 0) {
            // Silently skip empty files - they're likely from interrupted saves
            return null;
          }

          const content = await readFile(filePath, "utf-8");

          // Skip files that only contain whitespace
          if (content.trim().length === 0) {
            return null;
          }

          const parsed = JSON.parse(content) as RawMessageHistory;
          const result: SavedMessageHistory =
            parsed as unknown as SavedMessageHistory;
          // Basic validation - ensure messages array exists
          if (parsed && Array.isArray(parsed.messages)) {
            // Convert date strings back to Date objects
            result.createdAt = new Date(parsed.createdAt ?? 0);
            result.updatedAt = new Date(parsed.updatedAt ?? 0);
            return result;
          }
        } catch (error) {
          // Only log unexpected errors, not empty/malformed JSON files
          // which are common from interrupted saves
          if (
            !(error instanceof SyntaxError) ||
            !error.message.includes("Unexpected end of JSON input")
          ) {
            console.error(`Error reading or parsing file ${filePath}:`, error);
          }
        }
        return null; // Return null for failed reads/parses
      });

      const results = await Promise.all(fileReadPromises);

      // Filter out null results and return them (already sorted by file modification time)
      return results.filter(
        (result): result is SavedMessageHistory => result !== null,
      );
    } catch (error) {
      // Handle cases where the directory might not exist or other readdir errors
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
        console.error(`Error loading message history from ${stateDir}:`, error);
      }
      // Return empty array if directory doesn't exist or other read errors occur
      return [];
    }
  }

  // Method to restore state from a SavedMessageHistory object
  restore(savedHistory: SavedMessageHistory): void {
    this.sessionId = savedHistory.sessionId;
    this.modelId = savedHistory.modelId;
    this.title = savedHistory.title;
    // Ensure dates are Date objects, though load should handle this
    this.createdAt =
      typeof savedHistory.createdAt === "string"
        ? new Date(savedHistory.createdAt)
        : savedHistory.createdAt;
    this.updatedAt =
      typeof savedHistory.updatedAt === "string"
        ? new Date(savedHistory.updatedAt)
        : savedHistory.updatedAt;
    // Sanitize messages while preserving original order
    // Only sanitize assistant and tool messages (not user or system messages)
    const sanitizedMessages = savedHistory.messages.map((msg) => {
      if (msg.role === "assistant" || msg.role === "tool") {
        const [sanitized] = sanitizeResponseMessages([msg as ResponseMessage]);
        return sanitized;
      }
      return msg;
    });
    // Filter out messages with empty content arrays
    const validMessages = sanitizedMessages.filter(this.validMessage);
    this.history = [...validMessages];

    // Handle tokenUsage restoration with backward compatibility
    // Old format: TokenUsageTurn[] (array)
    // New format: SessionTokenUsage (object with total and lastTurn)
    if (savedHistory.tokenUsage) {
      if (Array.isArray(savedHistory.tokenUsage)) {
        // Old format: convert array to new format
        const oldUsage = savedHistory.tokenUsage as TokenUsageTurn[];
        if (oldUsage.length > 0) {
          const total = oldUsage.reduce(
            (acc, turn) => {
              acc.inputTokens += turn.inputTokens;
              acc.outputTokens += turn.outputTokens;
              acc.totalTokens += turn.totalTokens;
              acc.cachedInputTokens += turn.cachedInputTokens;
              acc.reasoningTokens += turn.reasoningTokens;
              acc.estimatedCost += turn.estimatedCost;
              return acc;
            },
            {
              inputTokens: 0,
              outputTokens: 0,
              totalTokens: 0,
              cachedInputTokens: 0,
              reasoningTokens: 0,
              estimatedCost: 0,
            },
          );
          const lastTurn = oldUsage[oldUsage.length - 1];
          this.tokenUsage = {
            total,
            lastTurn: {
              inputTokens: lastTurn.inputTokens,
              outputTokens: lastTurn.outputTokens,
              totalTokens: lastTurn.totalTokens,
              cachedInputTokens: lastTurn.cachedInputTokens,
              reasoningTokens: lastTurn.reasoningTokens,
              estimatedCost: lastTurn.estimatedCost,
            },
          };
        } else {
          this.tokenUsage = null;
        }
      } else {
        // New format: already SessionTokenUsage
        this.tokenUsage = savedHistory.tokenUsage as SessionTokenUsage;
      }
    } else {
      this.tokenUsage = null;
    }

    this.timing = savedHistory.timing ?? null;

    this.metadata = savedHistory.metadata ?? {};
  }

  // Token usage tracking methods
  recordTurnUsage(usage: {
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
  }): void {
    const modelConfig = this.modelManager.getModelMetadata("repl");
    const estimatedCost =
      usage.inputTokens * modelConfig.costPerInputToken +
      usage.outputTokens * modelConfig.costPerOutputToken;

    // Initialize tokenUsage if null
    if (this.tokenUsage === null) {
      this.tokenUsage = this.createEmptyTokenUsage();
    }

    // Update total
    this.tokenUsage.total.inputTokens += usage.inputTokens;
    this.tokenUsage.total.outputTokens += usage.outputTokens;
    this.tokenUsage.total.totalTokens += usage.totalTokens;
    this.tokenUsage.total.cachedInputTokens += usage.cachedInputTokens;
    this.tokenUsage.total.reasoningTokens += usage.reasoningTokens;
    this.tokenUsage.total.estimatedCost += estimatedCost;

    // Update lastTurn
    this.tokenUsage.lastTurn.inputTokens = usage.inputTokens;
    this.tokenUsage.lastTurn.outputTokens = usage.outputTokens;
    this.tokenUsage.lastTurn.totalTokens = usage.totalTokens;
    this.tokenUsage.lastTurn.cachedInputTokens = usage.cachedInputTokens;
    this.tokenUsage.lastTurn.reasoningTokens = usage.reasoningTokens;
    this.tokenUsage.lastTurn.estimatedCost = estimatedCost;
  }

  /**
   * @deprecated This method returns an empty array since we no longer store
   * per-turn usage data. Use getTotalTokenUsage() or getLastTurnContextWindow() instead.
   */
  getTokenUsage(): TokenUsageTurn[] {
    // Return empty array for backward compatibility
    // The actual data is now stored in compact format in this.tokenUsage
    return [];
  }

  getTotalTokenUsage(): {
    inputTokens: number;
    outputTokens: number;
    totalTokens: number;
    cachedInputTokens: number;
    reasoningTokens: number;
    estimatedCost: number;
  } {
    if (this.tokenUsage === null) {
      return {
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        cachedInputTokens: 0,
        reasoningTokens: 0,
        estimatedCost: 0,
      };
    }
    return { ...this.tokenUsage.total };
  }

  getLastTurnContextWindow(): number {
    if (this.tokenUsage === null) {
      return 0;
    }
    return this.tokenUsage.lastTurn.totalTokens;
  }

  clearTokenUsage(): void {
    this.tokenUsage = null;
  }

  recordTurnTiming(timing: {
    wallClockMs: number;
    modelMs: number;
    toolMs: number;
  }): void {
    if (this.timing === null) {
      this.timing = this.createEmptyTiming();
    }

    this.timing.total.wallClockMs += timing.wallClockMs;
    this.timing.total.modelMs += timing.modelMs;
    this.timing.total.toolMs += timing.toolMs;
    this.timing.total.turns += 1;

    this.timing.lastTurn.wallClockMs = timing.wallClockMs;
    this.timing.lastTurn.modelMs = timing.modelMs;
    this.timing.lastTurn.toolMs = timing.toolMs;
  }

  /**
   * Session-level rollup of timing data. Returns null when no timing has been
   * recorded for the session. `overheadMs` is non-tool, non-model wall-clock
   * time, and `toolTimeRatio` is the fraction of total wall-clock spent in tools.
   */
  getTimingSummary(): {
    wallClockMs: number;
    modelMs: number;
    toolMs: number;
    overheadMs: number;
    toolTimeRatio: number;
    turns: number;
  } | null {
    if (this.timing === null) {
      return null;
    }
    const { wallClockMs, modelMs, toolMs, turns } = this.timing.total;
    const overheadMs = Math.max(0, wallClockMs - modelMs - toolMs);
    const toolTimeRatio = wallClockMs > 0 ? toolMs / wallClockMs : 0;
    return {
      wallClockMs,
      modelMs,
      toolMs,
      overheadMs,
      toolTimeRatio,
      turns,
    };
  }
}
