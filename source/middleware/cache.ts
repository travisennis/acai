import { createHash } from "node:crypto";
import type { SharedV4ProviderOptions } from "@ai-sdk/provider";
import type { LanguageModelMiddleware } from "ai";
import { logger } from "../utils/logger.ts";

interface CacheOptions {
  ttl?: string;
  retention?: string;
}

function applyCaching(
  input: {
    providerOptions?: SharedV4ProviderOptions | undefined;
  },
  options: CacheOptions = {},
) {
  const { ttl } = options;

  input.providerOptions = {
    anthropic: {
      cacheControl: ttl ? { type: "ephemeral", ttl } : { type: "ephemeral" },
    },
    openrouter: {
      // biome-ignore lint/style/useNamingConvention: third-party
      cache_control: { type: "ephemeral" },
      cacheControl: { type: "ephemeral" },
    },
    bedrock: {
      cachePoint: { type: "ephemeral" },
    },
    openaiCompatible: {
      // biome-ignore lint/style/useNamingConvention: third-party
      cache_control: { type: "ephemeral" },
    },
  } as SharedV4ProviderOptions;
}

function generateCacheKey(text: string, salt?: string): string {
  const content = salt ? `${text}|${salt}` : text;
  return createHash("sha256").update(content).digest("hex").slice(0, 32);
}

function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

function getMinTokenThreshold(modelId: string): number {
  if (modelId?.includes("haiku")) return 4096;
  if (modelId?.includes("opus")) return 4096;
  if (modelId?.includes("sonnet")) return 1024;
  return 1024;
}

function isEligibleForCaching(text: string, modelId?: string): boolean {
  const tokenCount = estimateTokens(text);
  const minThreshold = getMinTokenThreshold(modelId ?? "");

  if (tokenCount < minThreshold) {
    return false;
  }

  return true;
}

function detectProvider(modelId: string): string {
  if (
    modelId.includes("sonnet") ||
    modelId.includes("opus") ||
    modelId.includes("haiku")
  ) {
    return "anthropic";
  }
  return "unknown";
}

export const cacheMiddleware: LanguageModelMiddleware = {
  specificationVersion: "v4",
  transformParams: async ({ params, model }) => {
    const modelId = model.modelId;
    const provider = detectProvider(modelId);

    if (provider === "unknown") {
      return params;
    }

    const msgs = params.prompt;

    // Extract system messages for cache key generation
    const systemMessages = msgs.filter((msg) => msg.role === "system");
    const systemText = systemMessages
      .map((msg) =>
        typeof msg.content === "string"
          ? msg.content
          : Array.isArray(msg.content)
            ? (msg.content as Array<{ text: string }>)
                .map((c) => c.text)
                .join(" ")
            : "",
      )
      .join("\n");

    // Check if system prompt is eligible for caching
    const isEligible = isEligibleForCaching(systemText, modelId);

    if (!isEligible) {
      return params;
    }

    // Generate deterministic cache key
    const cacheKey = generateCacheKey(systemText, provider);

    // Apply caching to system messages
    for (const systemMsg of systemMessages) {
      applyCaching(systemMsg, { ttl: "1h" });
    }

    // Get the last two user messages for caching
    const userMessages = msgs.filter((msg) => msg.role === "user");
    const lastTwoUserMessages = userMessages.slice(-2);

    // Mark both the latest and second-to-last user messages as ephemeral
    for (const userMessage of lastTwoUserMessages) {
      const content = userMessage.content;
      if (Array.isArray(content)) {
        const finalContent = content.at(-1);
        if (finalContent) {
          applyCaching(finalContent, { ttl: "1h" });
        }
      }
    }

    const tools = params.tools;
    if (tools) {
      const lastTool = tools.at(-1);
      if (lastTool?.type === "function") {
        applyCaching(lastTool, { ttl: "1h" });
      }
    }

    // Add cache metadata for observability
    params.providerOptions = params.providerOptions || {};
    params.providerOptions["__cacheMetadata"] = {
      cacheKey,
      provider,
      eligible: isEligible,
      systemTokens: estimateTokens(systemText),
      threshold: getMinTokenThreshold(modelId),
      timestamp: Date.now(),
    };

    logger.info(`[Cache] Applied caching for ${provider} model`);

    return params;
  },
};
