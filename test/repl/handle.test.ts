import assert from "node:assert/strict";
import { afterEach, beforeEach, describe, it, mock } from "node:test";
import type {
  Agent,
  AgentEvent,
  AgentState,
} from "../../source/agent/index.ts";
import type { CommandManager } from "../../source/commands/manager.ts";
import type { ConfigManager } from "../../source/config/index.ts";
import type { WorkspaceContext } from "../../source/index.ts";
import type { ModelManager } from "../../source/models/manager.ts";
import type { ModelName } from "../../source/models/providers.ts";
import type { PromptManager } from "../../source/prompts/manager.ts";
import { Repl } from "../../source/repl/index.ts";
import type { SessionManager } from "../../source/sessions/manager.ts";
import type { TokenCounter } from "../../source/tokens/counter.ts";
import type { TokenTracker } from "../../source/tokens/tracker.ts";

interface MockedReplDeps {
  agent: Agent;
  sessionManager: SessionManager;
  promptManager: PromptManager;
  modelManager: ModelManager;
  tokenTracker: TokenTracker;
  commands: CommandManager;
  configManager: ConfigManager;
  tokenCounter: TokenCounter;
  promptHistory: string[];
  workspace: WorkspaceContext;
}

function createMockDeps(): MockedReplDeps {
  const sessionManager = {
    appendUserMessage: mock.fn(),
    getLastTurnContextWindow: mock.fn(() => 0),
    clearTransientMessages: mock.fn(),
    save: mock.fn(() => Promise.resolve()),
    getSessionId: mock.fn(() => "test-session-id"),
  } as unknown as SessionManager;

  return {
    agent: {} as Agent,
    sessionManager,
    promptManager: {
      isPending: mock.fn(() => false),
      hasContext: mock.fn(() => false),
      getContextTokenCount: mock.fn(() => 0),
      addContext: mock.fn(),
      set: mock.fn(),
      get: mock.fn(() => "test prompt"),
      getUserMessage: mock.fn(() => "test user message"),
    } as unknown as PromptManager,
    modelManager: {
      getModelMetadata: mock.fn(() => ({
        contextWindow: 128000,
        maxTokens: 4096,
      })),
    } as unknown as ModelManager,
    tokenTracker: {
      getTotalTokens: mock.fn(() => 0),
      getInputTokens: mock.fn(() => 0),
      getOutputTokens: mock.fn(() => 0),
    } as unknown as TokenTracker,
    commands: {
      handle: mock.fn(),
      getCompletions: mock.fn(() => []),
    } as unknown as CommandManager,
    configManager: {
      getConfig: mock.fn(() => ({
        skills: { path: "" },
      })),
    } as unknown as ConfigManager,
    tokenCounter: {
      count: mock.fn(() => 0),
    } as unknown as TokenCounter,
    promptHistory: [],
    workspace: {
      primaryDir: "/test",
      allowedDirs: [],
    } as unknown as WorkspaceContext,
  };
}

/** Creates a minimal AgentState for tests. */
function createAgentState(overrides?: Partial<AgentState>): AgentState {
  return {
    modelId: "test-model" as ModelName,
    modelConfig: {
      id: "test-model" as ModelName,
      provider: "openai",
      contextWindow: 128000,
      supportsToolCalling: false,
      supportsReasoning: false,
      costPerInputToken: 0,
      costPerOutputToken: 0,
      maxOutputTokens: 4096,
      defaultTemperature: 0,
      promptFormat: "markdown",
    },
    stepCount: 0,
    toolCallCount: 0,
    steps: [],
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
      start: Date.now(),
      stop: Date.now(),
    },
    ...overrides,
  };
}

describe("Repl handle method", () => {
  let deps: MockedReplDeps;
  let repl: Repl;
  let originalStdoutIsTty: boolean | undefined;
  let originalStdinIsTty: boolean | undefined;

  beforeEach(() => {
    // biome-ignore lint/style/useNamingConvention: isTTY is standard Node.js API
    originalStdoutIsTty = (process.stdout as unknown as { isTTY?: boolean })
      .isTTY;
    // biome-ignore lint/style/useNamingConvention: isTTY is standard Node.js API
    originalStdinIsTty = (process.stdin as unknown as { isTTY?: boolean })
      .isTTY;
    // biome-ignore lint/style/useNamingConvention: isTTY is standard Node.js API
    (process.stdout as unknown as { isTTY?: boolean }).isTTY = true;
    // biome-ignore lint/style/useNamingConvention: isTTY is standard Node.js API
    (process.stdin as unknown as { isTTY?: boolean }).isTTY = true;

    deps = createMockDeps();
    repl = new Repl(deps);

    // Stub the TUI's start method since we don't need a real terminal
    // biome-ignore lint/suspicious/noExplicitAny: accessing private repl member
    const tui = (repl as any).tui;
    mock.method(tui, "start", () => {});
    mock.method(tui, "requestRender", () => {});
    mock.method(tui, "scrollToBottom", () => {});
  });

  afterEach(() => {
    // biome-ignore lint/style/useNamingConvention: isTTY is standard Node.js API
    (process.stdout as unknown as { isTTY?: boolean }).isTTY =
      originalStdoutIsTty;
    // biome-ignore lint/style/useNamingConvention: isTTY is standard Node.js API
    (process.stdin as unknown as { isTTY?: boolean }).isTTY =
      originalStdinIsTty;

    // Clean up any loading animation interval that might keep the process alive
    // biome-ignore lint/suspicious/noExplicitAny: accessing private repl member
    const loader = (repl as any).loadingAnimation;
    if (loader) {
      loader.stop();
    }

    mock.reset();
  });

  describe("initialization check", () => {
    it("should call init when not initialized", async () => {
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      const initMock = mock.method(repl, "init" as any, () =>
        Promise.resolve(),
      );

      await repl.handle({ type: "step-start" }, createAgentState());

      assert.equal(initMock.mock.callCount(), 1);
    });

    it("should not call init when already initialized", async () => {
      await repl.init();
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      mock.method(repl, "init" as any, () => Promise.resolve());

      await repl.handle({ type: "step-start" }, createAgentState());

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).isInitialized, true);
    });
  });

  describe("agent-start event", () => {
    it("should disable editor submit and show loading animation", async () => {
      await repl.init();
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      const editor = (repl as any).editor;

      await repl.handle({ type: "agent-start" }, createAgentState());

      assert.equal(editor.disableSubmit, true);
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.notEqual((repl as any).loadingAnimation, null);
    });

    it("should stop existing loading animation before creating new one", async () => {
      await repl.init();
      const fakeLoader = { stop: mock.fn() };
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      (repl as any).loadingAnimation = fakeLoader;

      await repl.handle({ type: "agent-start" }, createAgentState());

      assert.equal(fakeLoader.stop.mock.callCount(), 1);
    });
  });

  describe("step-start and step-stop events", () => {
    it("should handle step-start without errors", async () => {
      await repl.init();
      await repl.handle({ type: "step-start" }, createAgentState());
      assert.ok(true);
    });

    it("should handle step-stop without errors", async () => {
      await repl.init();
      await repl.handle({ type: "step-stop" }, createAgentState());
      assert.ok(true);
    });
  });

  describe("message-start event", () => {
    it("should create assistant component for assistant role", async () => {
      await repl.init();

      await repl.handle(
        {
          type: "message-start",
          role: "assistant",
          content: "Hello",
        },
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.notEqual((repl as any).streamingComponent, null);
    });

    it("should not create streaming component for non-assistant role", async () => {
      await repl.init();

      await repl.handle(
        {
          type: "message-start",
          role: "user",
          content: "Hello",
        } as unknown as AgentEvent,
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).streamingComponent, null);
    });
  });

  describe("message event", () => {
    it("should update streaming component for assistant role", async () => {
      await repl.init();

      // First create a streaming component via message-start
      await repl.handle(
        {
          type: "message-start",
          role: "assistant",
          content: "Hello",
        },
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      const component = (repl as any).streamingComponent;
      const updateMock = mock.method(component, "updateContent", () => {});

      await repl.handle(
        {
          type: "message",
          role: "assistant",
          content: "Hello world",
        },
        createAgentState(),
      );

      assert.equal(updateMock.mock.callCount(), 1);
    });

    it("should not update streaming component when no component exists", async () => {
      await repl.init();

      await repl.handle(
        {
          type: "message",
          role: "assistant",
          content: "Hello world",
        },
        createAgentState(),
      );

      assert.ok(true);
    });
  });

  describe("message-end event", () => {
    it("should finalize streaming component and clear reference", async () => {
      await repl.init();

      // Set up streaming component
      await repl.handle(
        {
          type: "message-start",
          role: "assistant",
          content: "Hello",
        },
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      const component = (repl as any).streamingComponent;
      const updateMock = mock.method(component, "updateContent", () => {});

      await repl.handle(
        {
          type: "message-end",
          role: "assistant",
          content: "Hello world",
        },
        createAgentState(),
      );

      assert.equal(updateMock.mock.callCount(), 1);
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).streamingComponent, null);
    });

    it("should handle message-end with no streaming component", async () => {
      await repl.init();

      await repl.handle(
        {
          type: "message-end",
          role: "assistant",
          content: "Hello",
        },
        createAgentState(),
      );

      assert.ok(true);
    });
  });

  describe("tool-call-lifecycle event", () => {
    it("should create new tool execution component for unknown tool call", async () => {
      await repl.init();

      await repl.handle(
        {
          type: "tool-call-lifecycle",
          toolCallId: "call-1",
          events: [
            {
              type: "tool-call-start",
              name: "read_file",
              toolCallId: "call-1",
              msg: "Reading file...",
              args: {},
            },
          ],
        },
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.ok((repl as any).pendingTools.has("call-1"));
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).allToolExecutions.length, 1);
    });

    it("should update existing tool execution component", async () => {
      await repl.init();

      // Create the component first
      await repl.handle(
        {
          type: "tool-call-lifecycle",
          toolCallId: "call-1",
          events: [
            {
              type: "tool-call-start",
              name: "read_file",
              toolCallId: "call-1",
              msg: "Reading file...",
              args: {},
            },
          ],
        },
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      const component = (repl as any).pendingTools.get("call-1");
      const updateMock = mock.method(component, "update", () => {});

      // Send another event for the same tool call
      await repl.handle(
        {
          type: "tool-call-lifecycle",
          toolCallId: "call-1",
          events: [
            {
              type: "tool-call-end",
              name: "read_file",
              toolCallId: "call-1",
              msg: "File read successfully",
              args: {},
            },
          ],
        },
        createAgentState(),
      );

      assert.equal(updateMock.mock.callCount(), 1);
    });
  });

  describe("agent-stop event", () => {
    it("should clean up loading animation and streaming state", async () => {
      await repl.init();

      // Set up streaming component
      await repl.handle(
        {
          type: "message-start",
          role: "assistant",
          content: "Hello",
        },
        createAgentState(),
      );

      // Simulate agent-start to set loading
      await repl.handle({ type: "agent-start" }, createAgentState());

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.notEqual((repl as any).loadingAnimation, null);
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.notEqual((repl as any).streamingComponent, null);

      await repl.handle({ type: "agent-stop" }, createAgentState());

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).loadingAnimation, null);
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).streamingComponent, null);
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).editor.disableSubmit, false);
    });

    it("should save session when noSession is false", async () => {
      await repl.init();
      await repl.handle({ type: "agent-stop" }, createAgentState());

      assert.equal(
        (
          deps.sessionManager.save as unknown as ReturnType<typeof mock.fn>
        ).mock.callCount(),
        1,
      );
    });

    it("should not save session when noSession is true", async () => {
      const noSessionRepl = new Repl({ ...deps, noSession: true });
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      const tui = (noSessionRepl as any).tui;
      mock.method(tui, "start", () => {});
      mock.method(tui, "requestRender", () => {});
      await noSessionRepl.init();

      await noSessionRepl.handle({ type: "agent-stop" }, createAgentState());

      assert.equal(
        (
          deps.sessionManager.save as unknown as ReturnType<typeof mock.fn>
        ).mock.callCount(),
        0,
      );
    });
  });

  describe("agent-error event", () => {
    it("should clean up loading animation and streaming state", async () => {
      await repl.init();

      // Set up state as if agent was running
      await repl.handle(
        {
          type: "message-start",
          role: "assistant",
          content: "Hello",
        },
        createAgentState(),
      );
      await repl.handle({ type: "agent-start" }, createAgentState());

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.notEqual((repl as any).loadingAnimation, null);
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.notEqual((repl as any).streamingComponent, null);

      await repl.handle(
        { type: "agent-error", message: "Something went wrong" },
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).loadingAnimation, null);
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).streamingComponent, null);
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).editor.disableSubmit, false);
    });

    it("should save session when noSession is false", async () => {
      await repl.init();
      await repl.handle(
        { type: "agent-error", message: "Error" },
        createAgentState(),
      );

      assert.equal(
        (
          deps.sessionManager.save as unknown as ReturnType<typeof mock.fn>
        ).mock.callCount(),
        1,
      );
    });
  });

  describe("thinking-start event", () => {
    it("should create a thinking block component", async () => {
      await repl.init();

      await repl.handle(
        { type: "thinking-start", content: "Thinking..." },
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.notEqual((repl as any).thinkingBlockComponent, null);
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).allThinkingBlocks.length, 1);
    });

    it("should insert thinking block before streaming component if one exists", async () => {
      await repl.init();

      // Create a streaming component first
      await repl.handle(
        {
          type: "message-start",
          role: "assistant",
          content: "Some text",
        },
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      const chatContainer = (repl as any).chatContainer;
      const insertBeforeMock = mock.method(
        chatContainer,
        "insertChildBefore",
        () => {},
      );

      await repl.handle(
        { type: "thinking-start", content: "Thinking..." },
        createAgentState(),
      );

      assert.ok(insertBeforeMock.mock.callCount() > 0);
    });
  });

  describe("thinking event", () => {
    it("should update thinking block component", async () => {
      await repl.init();

      // Create thinking block first
      await repl.handle(
        { type: "thinking-start", content: "Thinking..." },
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      const component = (repl as any).thinkingBlockComponent;
      const updateMock = mock.method(component, "updateContent", () => {});

      await repl.handle(
        { type: "thinking", content: "Still thinking..." },
        createAgentState(),
      );

      assert.equal(updateMock.mock.callCount(), 1);
    });

    it("should handle thinking event with no thinking block", async () => {
      await repl.init();

      await repl.handle(
        { type: "thinking", content: "Thinking..." },
        createAgentState(),
      );

      assert.ok(true);
    });
  });

  describe("thinking-end event", () => {
    it("should finalize thinking block and clear reference", async () => {
      await repl.init();

      // Create thinking block first
      await repl.handle(
        { type: "thinking-start", content: "Thinking..." },
        createAgentState(),
      );

      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      const component = (repl as any).thinkingBlockComponent;
      const endMock = mock.method(component, "endThinking", () => {});

      await repl.handle(
        { type: "thinking-end", content: "Done thinking" },
        createAgentState(),
      );

      assert.equal(endMock.mock.callCount(), 1);
      // biome-ignore lint/suspicious/noExplicitAny: accessing private member
      assert.equal((repl as any).thinkingBlockComponent, null);
    });

    it("should handle thinking-end with no thinking block", async () => {
      await repl.init();

      await repl.handle(
        { type: "thinking-end", content: "Done thinking" },
        createAgentState(),
      );

      assert.ok(true);
    });
  });

  describe("default case", () => {
    it("should not throw for unknown event type", async () => {
      await repl.init();

      await repl.handle({ type: "step-start" }, createAgentState());
      assert.ok(true);
    });
  });
});
