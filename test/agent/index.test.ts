/** biome-ignore-all lint/suspicious/noExplicitAny: mocks in test need any casts */

import assert from "node:assert/strict";
import { describe, it, mock } from "node:test";
import type { AgentEvent } from "../../source/agent/index.ts";
import {
  createMockConfig,
  createMockModelManager,
  createMockSessionManager,
  createMockTokenTracker,
} from "../utils/mocking.ts";

const USAGE_ZERO = {
  inputTokens: { total: 0, noCache: 0, cacheRead: 0, cacheWrite: 0 },
  outputTokens: { total: 0, text: 0, reasoning: 0 },
  totalTokens: 0,
};

/**
 * Mock language model that always returns a valid stream with text content
 * and finish reason "tool-calls" so the agent loop continues on success.
 */
function createWorkingMockModel(finishType: "tool-calls" | "stop") {
  return {
    provider: "test",
    modelId: "test-model",
    specificationVersion: "v3" as const,
    doGenerate: async () => ({
      content: [],
      finishReason: { unified: "stop" as const, original: "stop" },
      usage: USAGE_ZERO,
    }),
    doStream: async () => {
      const stream = new ReadableStream({
        start(controller) {
          controller.enqueue({ type: "text-start" as const, id: "0" });
          controller.enqueue({
            type: "text-delta" as const,
            id: "0",
            delta: "hello",
          });
          controller.enqueue({ type: "text-end" as const, id: "0" });
          controller.enqueue({
            type: "finish" as const,
            finishReason: { unified: finishType, original: finishType },
            usage: USAGE_ZERO,
            providerMetadata: undefined,
          });
          controller.close();
        },
      });
      return { stream };
    },
  };
}

describe("Agent consecutiveErrors", () => {
  it("resets consecutiveErrors after a successful iteration", async () => {
    const mockModel = createWorkingMockModel("tool-calls");

    const modelManager = createMockModelManager({
      contextWindow: 200000,
      supportsToolCalling: true,
    });
    (modelManager as any).getModel = mock.fn(() => mockModel);
    (modelManager as any).getModelMetadata = mock.fn(() => ({
      id: "test-model",
      provider: "test",
      contextWindow: 200000,
      supportsToolCalling: true,
      supportsReasoning: false,
      costPerInputToken: 0,
      costPerOutputToken: 0,
      maxOutputTokens: 8192,
      defaultTemperature: 0.7,
      promptFormat: "markdown",
    }));

    const config = createMockConfig();
    const tokenTracker = createMockTokenTracker();
    (tokenTracker as any).trackUsage = mock.fn();

    const sessionManager = createMockSessionManager([
      { role: "user", content: [{ type: "text", text: "hello" }] },
    ]);
    sessionManager.getSessionId = mock.fn(() => "test-session") as any;
    (sessionManager as any).setContextWindow = mock.fn();
    (sessionManager as any).recordTurnUsage = mock.fn();

    // Inject transient errors on specific appendResponseMessages calls.
    // These are regular Errors, not NoOutputGeneratedError, so the agent's
    // catch block treats them as recoverable.
    let appendCallCount = 0;
    sessionManager.appendResponseMessages = mock.fn(() => {
      appendCallCount++;
      if (appendCallCount === 1 || appendCallCount === 3) {
        throw new Error("Transient session error");
      }
    }) as any;

    const agent = new (await import("../../source/agent/index.ts")).Agent({
      config: config as any,
      modelManager,
      tokenTracker,
      sessionManager,
    });

    const events: AgentEvent[] = [];
    for await (const event of agent.run({
      systemPrompt: "test",
      input: "test input",
      tools: {} as any,
      maxIterations: 10,
      maxRetries: 1, // 2 consecutive errors would abort the loop
    })) {
      events.push(event);
    }

    // If the bug is present, the sequence Error→Success→Error causes
    // consecutiveErrors to grow to 2 > maxRetries(1), aborting with
    // "Exceeded maximum retry attempts".
    const retryExceededEvents = events.filter(
      (e): e is AgentEvent & { type: "agent-error" } =>
        e.type === "agent-error" &&
        "message" in e &&
        typeof e.message === "string" &&
        e.message.includes("Exceeded maximum retry attempts"),
    );
    assert.equal(
      retryExceededEvents.length,
      0,
      "Should not abort when errors are separated by successful iterations",
    );
  });
});
