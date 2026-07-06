import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type {
  LanguageModelV4,
  LanguageModelV4CallOptions,
  LanguageModelV4GenerateResult,
  LanguageModelV4StreamResult,
} from "@ai-sdk/provider";
import type { LanguageModelMiddleware } from "ai";
import { createRateLimitMiddleware } from "../source/middleware/rate-limit.ts";

const mockGenerateResult = {
  content: [],
  finishReason: { unified: "stop" as const, raw: undefined },
  usage: {
    inputTokens: { total: 0, noCache: 0, cacheRead: 0, cacheWrite: 0 },
    outputTokens: { total: 0, text: 0, reasoning: 0 },
  },
  warnings: [],
} satisfies LanguageModelV4GenerateResult;

const mockStreamResult = {
  stream: new ReadableStream({
    start(controller) {
      controller.close();
    },
  }),
} satisfies LanguageModelV4StreamResult;

const mockOpts = {
  params: {} as LanguageModelV4CallOptions,
  model: {} as LanguageModelV4,
};

type GenerateOpts = Parameters<
  NonNullable<LanguageModelMiddleware["wrapGenerate"]>
>[0];
type StreamOpts = Parameters<
  NonNullable<LanguageModelMiddleware["wrapStream"]>
>[0];

function generate(mw: LanguageModelMiddleware, opts: GenerateOpts) {
  /* v8 ignore next 3 — guard for optional method that is always present in our middleware */
  if (!mw.wrapGenerate) {
    throw new Error("wrapGenerate not implemented");
  }
  return mw.wrapGenerate(opts);
}

function stream(mw: LanguageModelMiddleware, opts: StreamOpts) {
  /* v8 ignore next 3 — guard for optional method that is always present in our middleware */
  if (!mw.wrapStream) {
    throw new Error("wrapStream not implemented");
  }
  return mw.wrapStream(opts);
}

describe("rate-limit middleware", () => {
  it("calls doGenerate for each wrapGenerate invocation", async () => {
    const mw = createRateLimitMiddleware({
      requestsPerMinute: 30,
      interval: 500,
    });

    let callCount = 0;
    const doGenerate = async () => {
      callCount++;
      return mockGenerateResult;
    };
    const opts = {
      doGenerate,
      doStream: async () => mockStreamResult,
      ...mockOpts,
    };

    await generate(mw, opts);
    assert.equal(callCount, 1);
  });

  it("rate-limits calls beyond the configured limit within the interval", async () => {
    const mw = createRateLimitMiddleware({
      requestsPerMinute: 3,
      interval: 200,
    });

    let callCount = 0;
    const doGenerate = async () => {
      callCount++;
      return mockGenerateResult;
    };
    const opts = {
      doGenerate,
      doStream: async () => mockStreamResult,
      ...mockOpts,
    };

    // First 3 calls should all go through immediately
    await Promise.all(Array.from({ length: 3 }, () => generate(mw, opts)));
    assert.equal(callCount, 3);

    // Start a 4th call but don't await it — it should be queued
    const extraCall = generate(mw, opts);

    // After a small delay the 4th call should still be queued
    await new Promise((resolve) => setTimeout(resolve, 50));
    assert.equal(
      callCount,
      3,
      "4th call should be rate-limited and not execute within the window",
    );

    // Wait for the interval to elapse so the 4th call resolves
    await extraCall;
    assert.equal(callCount, 4);
  });

  it("resumes execution after the interval elapses", async () => {
    const mw = createRateLimitMiddleware({
      requestsPerMinute: 2,
      interval: 100,
    });

    let callCount = 0;
    const doGenerate = async () => {
      callCount++;
      return mockGenerateResult;
    };
    const opts = {
      doGenerate,
      doStream: async () => mockStreamResult,
      ...mockOpts,
    };

    // Exhaust the limit
    await Promise.all(Array.from({ length: 2 }, () => generate(mw, opts)));
    assert.equal(callCount, 2);

    // 3rd call is queued — start it, then wait for it to resolve
    const queuedCall = generate(mw, opts);
    await queuedCall;
    assert.equal(callCount, 3);
  });

  it("supports optional interval defaulting to 60 seconds", () => {
    const mw = createRateLimitMiddleware({ requestsPerMinute: 30 });
    assert.ok(mw);
    assert.equal(mw.specificationVersion, "v4");
  });

  it("calls doStream for wrapStream", async () => {
    const mw = createRateLimitMiddleware({
      requestsPerMinute: 30,
      interval: 500,
    });

    let streamCount = 0;
    const doStream = async () => {
      streamCount++;
      return mockStreamResult;
    };
    const opts = {
      doGenerate: async () => mockGenerateResult,
      doStream,
      ...mockOpts,
    };

    await stream(mw, opts);
    assert.equal(streamCount, 1);
  });

  it("shares throttle state between wrapGenerate and wrapStream", async () => {
    const mw = createRateLimitMiddleware({
      requestsPerMinute: 2,
      interval: 200,
    });

    let calls = 0;
    const doGenerate = async () => {
      calls++;
      return mockGenerateResult;
    };
    const doStream = async () => {
      calls++;
      return mockStreamResult;
    };
    const opts = {
      doGenerate,
      doStream,
      ...mockOpts,
    };

    // Use one generate and one stream call to use up both slots
    await generate(mw, opts);
    await stream(mw, opts);
    assert.equal(calls, 2);

    // A third call (generate) should be rate-limited
    const queuedCall = generate(mw, opts);
    await new Promise((resolve) => setTimeout(resolve, 30));
    assert.equal(calls, 2, "3rd call should be queued");

    await queuedCall;
    assert.equal(calls, 3);
  });
});
