import type { LanguageModelMiddleware } from "ai";
import pThrottle from "p-throttle";

export const createRateLimitMiddleware = ({
  requestsPerMinute,
  interval,
}: {
  requestsPerMinute: number;
  interval?: number;
}): LanguageModelMiddleware => {
  const throttle = pThrottle({
    limit: requestsPerMinute,
    interval: interval ?? 60 * 1000, // default 1 minute
  });

  // Create a single throttled wrapper shared across all invocations.
  // This ensures throttle state (queue, counters) persists between
  // calls, unlike creating throttle(fn) inside wrapGenerate/wrapStream
  // which would create a fresh rate-limiter on every model invocation.
  // biome-ignore lint/suspicious/noExplicitAny: p-throttle uses AnyFunction internally; a concrete type loses generic inference.
  const throttledCall = throttle(async (thunk: () => any) => thunk());

  return {
    specificationVersion: "v4",
    wrapGenerate: ({ doGenerate }) => {
      return throttledCall(() => doGenerate());
    },
    wrapStream: ({ doStream }) => {
      return throttledCall(() => doStream());
    },
  };
};
