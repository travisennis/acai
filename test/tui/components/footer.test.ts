import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { AgentState } from "../../../source/agent/index.ts";
import type { ModelManager } from "../../../source/models/manager.ts";
import type { ModelName } from "../../../source/models/providers.ts";
import { FooterComponent } from "../../../source/tui/components/footer.ts";

function createAgentState(): AgentState {
  return {
    modelId: "test-model" as ModelName,
    modelConfig: {
      id: "test-model" as ModelName,
      provider: "openai",
      contextWindow: 128000,
      supportsToolCalling: true,
      supportsReasoning: false,
      costPerInputToken: 0,
      costPerOutputToken: 0,
      maxOutputTokens: 4096,
      defaultTemperature: 0,
      promptFormat: "markdown",
    },
    stepCount: 20,
    toolCallCount: 12,
    steps: Array.from({ length: 10 }, () => ({
      toolCalls: [{ toolName: "test_tool" }],
      toolResults: [{ toolName: "test_tool" }],
    })),
    usage: {
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      cachedInputTokens: 0,
      reasoningTokens: 0,
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
      cachedInputTokens: 0,
      reasoningTokens: 0,
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
    timestamps: {
      start: 0,
      stop: 1000,
    },
  };
}

describe("FooterComponent", () => {
  it("renders aggregate agent step and tool-call counters", () => {
    const footer = new FooterComponent(
      {
        getModelMetadata: () => ({
          contextWindow: 128000,
          costPerInputToken: 0,
          costPerOutputToken: 0,
        }),
      } as unknown as ModelManager,
      undefined,
      {
        projectStatus: {
          path: "/test",
          isGitRepository: false,
          hasChanges: false,
          unpushedCommits: 0,
          fileChanges: {
            added: 0,
            modified: 0,
            deleted: 0,
            untracked: 0,
          },
          diffStats: {
            insertions: 0,
            deletions: 0,
          },
        },
        currentContextWindow: 0,
        contextWindow: 128000,
        agentState: createAgentState(),
      },
    );

    const output = footer.render(80).join("\n");

    assert.match(output, /Steps: 20 - Tool calls: 12 -/);
  });
});
