import type { LanguageModelMiddleware } from "ai";
import pThrottle from "p-throttle";

export const createRateLimitMiddleware = ({
  requestsPerMinute,
}: {
  requestsPerMinute: number;
}): LanguageModelMiddleware => {
  const throttle = pThrottle({
    limit: requestsPerMinute,
    interval: 60 * 1000, // 1 minute
  });

  return {
    specificationVersion: "v4",
    wrapGenerate: ({ doGenerate }) => {
      const throttledGenerate = throttle(doGenerate);
      return Promise.resolve(throttledGenerate());
    },
    wrapStream: ({ doStream }) => {
      const throttledStream = throttle(doStream);
      return throttledStream();
    },
  };
};
