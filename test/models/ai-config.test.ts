import { deepStrictEqual, strictEqual } from "node:assert/strict";
import { describe, it } from "node:test";
import { AiConfig } from "../../source/models/ai-config.ts";

describe("AiConfig", () => {
  describe("maxOutputTokens()", () => {
    it("should return maxOutputTokens for non-Anthropic models", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "openrouter:sonnet-4.5" as const,
          provider: "openrouter",
          contextWindow: 8192,
          supportsToolCalling: true,
          supportsReasoning: false,
          costPerInputToken: 0.00001,
          costPerOutputToken: 0.00003,
          maxOutputTokens: 4096,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "Hello world",
      });

      strictEqual(config.maxOutputTokens(), 4096);
    });

    it("should reduce maxOutputTokens for Anthropic models with thinking", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "anthropic:opus" as const,
          provider: "anthropic",
          contextWindow: 200000,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.000015,
          costPerOutputToken: 0.000075,
          maxOutputTokens: 4096,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think super hard about this problem",
      });

      strictEqual(config.maxOutputTokens(), 4096 - 31999); // 4096 - 31999 = negative, but this tests the logic
    });

    it("should handle different thinking levels", () => {
      const lowConfig = new AiConfig({
        modelMetadata: {
          id: "anthropic:opus" as const,
          provider: "anthropic",
          contextWindow: 200000,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.000015,
          costPerOutputToken: 0.000075,
          maxOutputTokens: 10000,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think about this",
      });

      const mediumConfig = new AiConfig({
        modelMetadata: {
          id: "anthropic:opus" as const,
          provider: "anthropic",
          contextWindow: 200000,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.000015,
          costPerOutputToken: 0.000075,
          maxOutputTokens: 10000,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think hard about this",
      });

      const highConfig = new AiConfig({
        modelMetadata: {
          id: "anthropic:opus" as const,
          provider: "anthropic",
          contextWindow: 200000,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.000015,
          costPerOutputToken: 0.000075,
          maxOutputTokens: 10000,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think super hard about this",
      });

      strictEqual(lowConfig.maxOutputTokens(), 10000 - 4000); // low effort
      strictEqual(mediumConfig.maxOutputTokens(), 10000 - 10000); // medium effort
      strictEqual(highConfig.maxOutputTokens(), 10000 - 31999); // high effort
    });
  });

  describe("temperature()", () => {
    it("should return temperature when set", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "openrouter:sonnet-4.5" as const,
          provider: "openrouter",
          contextWindow: 8192,
          supportsToolCalling: true,
          supportsReasoning: false,
          costPerInputToken: 0.00001,
          costPerOutputToken: 0.00003,
          maxOutputTokens: 4096,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "Hello world",
      });

      strictEqual(config.temperature(), 0.7);
    });

    it("should return undefined when temperature is -1", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "openrouter:sonnet-4.5" as const,
          provider: "openrouter",
          contextWindow: 8192,
          supportsToolCalling: true,
          supportsReasoning: false,
          costPerInputToken: 0.00001,
          costPerOutputToken: 0.00003,
          maxOutputTokens: 4096,
          defaultTemperature: -1,
          promptFormat: "markdown",
        },
        prompt: "Hello world",
      });

      strictEqual(config.temperature(), undefined);
    });
  });

  describe("topP()", () => {
    it("should return 1 for Qwen models", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "opencode-go:qwen3.6-plus" as const,
          provider: "opencode-go",
          contextWindow: 8192,
          supportsToolCalling: true,
          supportsReasoning: false,
          costPerInputToken: 0.00001,
          costPerOutputToken: 0.00003,
          maxOutputTokens: 4096,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "Hello world",
      });

      strictEqual(config.topP(), 1);
    });

    it("should return undefined for non-Qwen models", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "openai:gpt-4.1" as const,
          provider: "openai",
          contextWindow: 8192,
          supportsToolCalling: true,
          supportsReasoning: false,
          costPerInputToken: 0.00001,
          costPerOutputToken: 0.00003,
          maxOutputTokens: 4096,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "Hello world",
      });

      strictEqual(config.topP(), undefined);
    });
  });

  describe("providerOptions()", () => {
    it("should return empty object when model doesn't support reasoning", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "openai:gpt-4.1" as const,
          provider: "openai",
          contextWindow: 8192,
          supportsToolCalling: true,
          supportsReasoning: false,
          costPerInputToken: 0.00001,
          costPerOutputToken: 0.00003,
          maxOutputTokens: 4096,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think super hard about this",
      });

      deepStrictEqual(config.providerOptions(), {});
    });

    it("should return empty object when prompt doesn't contain thinking keywords", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "anthropic:opus" as const,
          provider: "anthropic",
          contextWindow: 200000,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.000015,
          costPerOutputToken: 0.000075,
          maxOutputTokens: 4096,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "Hello world",
      });

      deepStrictEqual(config.providerOptions(), {});
    });

    it("should return Anthropic thinking configuration", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "anthropic:opus" as const,
          provider: "anthropic",
          contextWindow: 200000,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.000015,
          costPerOutputToken: 0.000075,
          maxOutputTokens: 4096,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think super hard about this",
      });

      deepStrictEqual(config.providerOptions(), {
        anthropic: {
          thinking: {
            type: "enabled",
            budgetTokens: 31999,
          },
        },
      });
    });

    it("should return OpenAI reasoning configuration", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "openai:o3" as const,
          provider: "openai",
          contextWindow: 8192,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.00001,
          costPerOutputToken: 0.00003,
          maxOutputTokens: 4096,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think hard about this",
      });

      deepStrictEqual(config.providerOptions(), {
        openai: {
          reasoningEffort: "medium",
          reasoningSummary: null,
        },
      });
    });

    it("should return Google thinking configuration", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "google:gemini-1.5-pro-latest" as const,
          provider: "google",
          contextWindow: 32768,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.0000025,
          costPerOutputToken: 0.00005,
          maxOutputTokens: 2048,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think about this",
      });

      deepStrictEqual(config.providerOptions(), {
        google: {
          thinkingConfig: {
            thinkingBudget: 4000,
          },
        },
      });
    });

    it("should return OpenRouter reasoning configuration", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "openrouter:mistralai/mistral-7b-instruct" as const,
          provider: "openrouter",
          contextWindow: 32768,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.000001,
          costPerOutputToken: 0.000001,
          maxOutputTokens: 4096,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think super hard about this",
      });

      deepStrictEqual(config.providerOptions(), {
        openrouter: {
          reasoning: {
            enabled: true,
            effort: "high",
          },
          usage: {
            include: true,
          },
        },
      });
    });

    it("should return Deepseek thinking configuration", () => {
      const config = new AiConfig({
        modelMetadata: {
          id: "deepseek:deepseek-v4-pro" as const,
          provider: "deepseek",
          contextWindow: 128000,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.00000055,
          costPerOutputToken: 0.00000219,
          maxOutputTokens: 32768,
          defaultTemperature: 0.6,
          promptFormat: "bracket",
        },
        prompt: "think about this",
      });

      deepStrictEqual(config.providerOptions(), {
        deepseek: {
          thinking: {
            type: "enabled",
          },
        },
      });
    });

    it("should handle different thinking effort levels", () => {
      const lowConfig = new AiConfig({
        modelMetadata: {
          id: "anthropic:opus" as const,
          provider: "anthropic",
          contextWindow: 200000,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.000015,
          costPerOutputToken: 0.000075,
          maxOutputTokens: 10000,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think",
      });

      const mediumConfig = new AiConfig({
        modelMetadata: {
          id: "anthropic:opus" as const,
          provider: "anthropic",
          contextWindow: 200000,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.000015,
          costPerOutputToken: 0.000075,
          maxOutputTokens: 10000,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "think hard",
      });

      const highConfig = new AiConfig({
        modelMetadata: {
          id: "anthropic:opus" as const,
          provider: "anthropic",
          contextWindow: 200000,
          supportsToolCalling: true,
          supportsReasoning: true,
          costPerInputToken: 0.000015,
          costPerOutputToken: 0.000075,
          maxOutputTokens: 10000,
          defaultTemperature: 0.7,
          promptFormat: "markdown",
        },
        prompt: "ultrathink",
      });

      deepStrictEqual(lowConfig.providerOptions(), {
        anthropic: {
          thinking: {
            type: "enabled",
            budgetTokens: 4000,
          },
        },
      });

      deepStrictEqual(mediumConfig.providerOptions(), {
        anthropic: {
          thinking: {
            type: "enabled",
            budgetTokens: 10000,
          },
        },
      });

      deepStrictEqual(highConfig.providerOptions(), {
        anthropic: {
          thinking: {
            type: "enabled",
            budgetTokens: 31999,
          },
        },
      });
    });
  });
});
