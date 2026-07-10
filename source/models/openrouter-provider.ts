import { createOpenResponses } from "@ai-sdk/open-responses";
import { createOpenAICompatible } from "@ai-sdk/openai-compatible";
import type { ProviderV2 } from "@ai-sdk/provider";
import { objectKeys } from "@travisennis/stdlib/object";
import { customProvider } from "ai";
import type { ModelMetadata } from "./providers.ts";

// Original OpenAI-compatible client for most models
const openRouterClient = createOpenAICompatible({
  name: "openrouter",
  apiKey: process.env["OPENROUTER_API_KEY"] ?? "",
  // biome-ignore lint/style/useNamingConvention: third-party controlled
  baseURL: "https://openrouter.ai/api/v1",
  headers: {
    "HTTP-Referer": "https://github.com/travisennis/acai",
    "X-Title": "acai",
  },
});

// Open Responses client for GPT models
const openResponses = createOpenResponses({
  name: "openrouter",
  url: "https://openrouter.ai/api/v1/responses",
  apiKey: process.env["OPENROUTER_API_KEY"] ?? "",
  headers: {
    "HTTP-Referer": "https://github.com/travisennis/acai",
    "X-Title": "acai",
  },
});

// Models using OpenAI-compatible API
const openrouterModels = {
  "minimax-m2-5": openRouterClient("minimax/minimax-m2.5"),
  "minimax-m2-7": openRouterClient("minimax/minimax-m2.7"),
  "sonnet-4.5": openRouterClient("anthropic/claude-sonnet-4.5"),
  "opus-4.6": openRouterClient("anthropic/claude-opus-4.6"),
  "owl-alpha": openRouterClient("openrouter/owl-alpha"),
  "haiku-4.5": openRouterClient("anthropic/claude-haiku-4.5"),
  "kimi-k2-5": openRouterClient("moonshotai/kimi-k2.5"),
  "kimi-k2-6": openRouterClient("moonshotai/kimi-k2.6"),
  "glm-5": openRouterClient("z-ai/glm-5"),
  "glm-5-1": openRouterClient("z-ai/glm-5.1"),
};

// Models using Open Responses API (GPT models)
const openResponsesModels = {
  "gpt-oss-120b": openResponses("openai/gpt-oss-120b:exacto"),
  "gpt-5-3-codex": openResponses("openai/gpt-5.3-codex"),
};

const allModels = { ...openrouterModels, ...openResponsesModels };

type ModelName = `openrouter:${keyof typeof allModels}`;

export const openrouterModelNames: ModelName[] = objectKeys(allModels).map(
  (key) => `openrouter:${key}` as const,
);

export const openrouterProvider = {
  openrouter: customProvider({
    languageModels: allModels,
    fallbackProvider: openRouterClient as unknown as ProviderV2,
  }),
};

export const openrouterModelRegistry: {
  [K in ModelName]: ModelMetadata<ModelName>;
} = {
  "openrouter:minimax-m2-5": {
    id: "openrouter:minimax-m2-5",
    provider: "openrouter",
    contextWindow: 204800,
    maxOutputTokens: 131072,
    defaultTemperature: 0.5,
    promptFormat: "markdown",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 3e-7,
    costPerOutputToken: 0.0000012,
  },
  "openrouter:minimax-m2-7": {
    id: "openrouter:minimax-m2-7",
    provider: "openrouter",
    contextWindow: 204800,
    maxOutputTokens: 131072,
    defaultTemperature: 0.5,
    promptFormat: "markdown",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 3e-7,
    costPerOutputToken: 0.0000012,
  },
  "openrouter:opus-4.6": {
    id: "openrouter:opus-4.6",
    provider: "openrouter",
    contextWindow: 200000,
    maxOutputTokens: 32000,
    defaultTemperature: 1.0,
    promptFormat: "markdown",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 0.000005,
    costPerOutputToken: 0.000025,
  },
  "openrouter:owl-alpha": {
    id: "openrouter:owl-alpha",
    provider: "openrouter",
    contextWindow: 1048756,
    maxOutputTokens: 262144,
    defaultTemperature: 0.5,
    promptFormat: "markdown",
    supportsReasoning: false,
    supportsToolCalling: true,
    costPerInputToken: 0,
    costPerOutputToken: 0,
  },
  "openrouter:sonnet-4.5": {
    id: "openrouter:sonnet-4.5",
    provider: "openrouter",
    contextWindow: 1000000,
    maxOutputTokens: 64000,
    defaultTemperature: 1.0,
    promptFormat: "markdown",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 0.000003,
    costPerOutputToken: 0.000015,
  },
  "openrouter:kimi-k2-5": {
    id: "openrouter:kimi-k2-5",
    provider: "openrouter",
    contextWindow: 262144,
    maxOutputTokens: 262144,
    defaultTemperature: 1.0,
    promptFormat: "markdown",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 6e-7,
    costPerOutputToken: 0.000003,
  },
  "openrouter:kimi-k2-6": {
    id: "openrouter:kimi-k2-6",
    provider: "openrouter",
    contextWindow: 256000,
    maxOutputTokens: 65536,
    defaultTemperature: 1.0,
    promptFormat: "markdown",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 7.448e-7,
    costPerOutputToken: 0.000004655,
  },
  "openrouter:glm-5": {
    id: "openrouter:glm-5",
    provider: "openrouter",
    contextWindow: 204800,
    maxOutputTokens: 64800,
    defaultTemperature: 0.5,
    promptFormat: "markdown",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 0.000001,
    costPerOutputToken: 0.0000032,
  },
  "openrouter:glm-5-1": {
    id: "openrouter:glm-5-1",
    provider: "openrouter",
    contextWindow: 202752,
    maxOutputTokens: 202752,
    defaultTemperature: 0.5,
    promptFormat: "markdown",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 0.0000014,
    costPerOutputToken: 0.0000044,
  },
  "openrouter:gpt-oss-120b": {
    id: "openrouter:gpt-oss-120b",
    provider: "openrouter",
    contextWindow: 131072,
    maxOutputTokens: 64000,
    defaultTemperature: 1.0,
    promptFormat: "xml",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 0.00000007256312,
    costPerOutputToken: 0.0000002903936,
  },
  "openrouter:haiku-4.5": {
    id: "openrouter:haiku-4.5",
    provider: "openrouter",
    contextWindow: 200000,
    maxOutputTokens: 32000,
    defaultTemperature: 1.0,
    promptFormat: "markdown",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 0.000001,
    costPerOutputToken: 0.000005,
  },
  "openrouter:gpt-5-3-codex": {
    id: "openrouter:gpt-5-3-codex",
    provider: "openrouter",
    contextWindow: 400000,
    maxOutputTokens: 128000,
    defaultTemperature: -1,
    promptFormat: "xml",
    supportsReasoning: true,
    supportsToolCalling: true,
    costPerInputToken: 0.00000175,
    costPerOutputToken: 0.000014,
  },
};
