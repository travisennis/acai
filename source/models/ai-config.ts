import type { SharedV4ProviderOptions } from "@ai-sdk/provider";
import type { ModelMetadata } from "./providers.ts";

type Effort = "none" | "low" | "medium" | "high";

const THINKING_TIERS: {
  pattern: RegExp;
  budget: number;
  effort: Effort;
}[] = [
  {
    pattern:
      /\b(ultrathink|think super hard|think really hard|think intensely)\b/i,
    budget: 31999,
    effort: "high",
  },
  {
    pattern: /\b(megathink|think (very )?hard|think (a lot|more|about it))\b/i,
    budget: 10000,
    effort: "medium",
  },
  {
    pattern: /\bthink\b/i, // Catch-all for standalone "think"
    budget: 4000,
    effort: "low",
  },
];

function calculateThinkingLevel(userInput: string): {
  tokenBudget: number;
  effort: Effort;
} {
  let tokenBudget = 0; // Default
  let effort: Effort = "none";
  for (const tier of THINKING_TIERS) {
    if (tier.pattern.test(userInput)) {
      tokenBudget = tier.budget;
      effort = tier.effort;
      break; // Use highest priority match
    }
  }
  return { tokenBudget, effort };
}

export class AiConfig {
  private modelMetadata: ModelMetadata;
  private prompt: string;
  private thinkingLevel: { tokenBudget: number; effort: Effort };

  constructor({
    modelMetadata,
    prompt,
  }: { modelMetadata: ModelMetadata; prompt: string }) {
    this.modelMetadata = modelMetadata;
    this.prompt = prompt;
    this.thinkingLevel = calculateThinkingLevel(this.prompt);
  }

  maxOutputTokens() {
    const modelConfig = this.modelMetadata;
    const maxTokens =
      modelConfig.provider === "anthropic" && modelConfig.supportsReasoning
        ? modelConfig.maxOutputTokens - this.thinkingLevel.tokenBudget
        : modelConfig.maxOutputTokens;
    return modelConfig.provider === "opencode" ? 8000 : maxTokens;
  }

  temperature() {
    const temp = this.modelMetadata.defaultTemperature;
    return temp > -1 ? temp : undefined;
  }

  topP() {
    const modelId = this.modelMetadata.id;
    if (modelId.toLowerCase().includes("qwen")) return 1;
    return undefined;
  }

  providerOptions(): SharedV4ProviderOptions {
    const modelConfig = this.modelMetadata;
    const thinkingLevel = this.thinkingLevel;

    const meta: SharedV4ProviderOptions = {
      [modelConfig.provider]: {},
    };

    if (modelConfig.supportsReasoning && thinkingLevel.effort !== "none") {
      switch (modelConfig.provider) {
        case "anthropic":
          Object.assign(meta["anthropic"], {
            thinking: {
              type: "enabled",
              budgetTokens: thinkingLevel.tokenBudget,
            },
          });
          break;
        case "openai":
          Object.assign(meta["openai"], {
            reasoningEffort: thinkingLevel.effort,
            reasoningSummary: null,
          });
          break;
        case "google":
          Object.assign(meta["google"], {
            thinkingConfig: {
              thinkingBudget: thinkingLevel.tokenBudget,
            },
          });
          break;
        case "openrouter":
          Object.assign(meta["openrouter"], {
            reasoning: {
              enabled: true,
              effort: thinkingLevel.effort,
            },
          });
          break;
        case "deepseek":
          Object.assign(meta["deepseek"], {
            thinking: {
              type: "enabled",
            },
          });
          break;
      }
    }

    switch (modelConfig.provider) {
      case "openrouter":
        Object.assign(meta["openrouter"], {
          usage: {
            include: true,
          },
        });
        break;
    }

    if (Object.keys(meta[modelConfig.provider]).length > 0) {
      return meta;
    }
    return {};
  }
}
