import { z } from "zod";
import type {
  ProcessSessionManager,
  SessionReadResult,
} from "../execution/process-session.ts";
import style from "../terminal/style.ts";
import { convertNullString } from "../utils/zod.ts";
import { truncateOutput } from "./bash.ts";
import type { ToolExecutionOptions } from "./types.ts";

export const BashSessionTool = {
  name: "BashSession" as const,
};

const toolDescription =
  "Interact with a still-running command session created by the Bash tool. When a Bash command outlives its yield window (or was started with background: true), Bash returns a session id like `bash_a1b2c3` instead of killing the command. Use this tool with that id to: poll for new output (omit input), send text to the process's stdin (set input; include a trailing newline to submit a line), or terminate it (set kill: true). Each call returns only output produced since the previous call, and returns as soon as there is new output or the process exits. Poll repeatedly until the result footer shows `[exit:N]`.";

// Default and maximum time to wait for new output or exit before returning
const DEFAULT_WAIT = 10 * 1000;
const MAX_WAIT = 120 * 1000;

const inputSchema = z.object({
  sessionId: z
    .string()
    .describe("The session id returned by the Bash tool (e.g. bash_a1b2c3)."),
  input: z
    .preprocess((val) => convertNullString(val), z.string().nullable())
    .describe(
      "Text to write to the session's stdin exactly as given; include a trailing newline to submit a line. Required but nullable. If null, this call only polls for output.",
    ),
  wait: z
    .preprocess((val) => convertNullString(val), z.coerce.number().nullable())
    .describe(
      `Milliseconds to wait for new output or process exit before returning. Required but nullable. If null, the default is ${DEFAULT_WAIT}ms; capped at ${MAX_WAIT}ms. The call returns as soon as there is anything to report.`,
    ),
  kill: z
    .boolean()
    .optional()
    .describe(
      "If true, terminate the session's process group (SIGTERM) and return any remaining output.",
    ),
});

type BashSessionInputSchema = z.infer<typeof inputSchema>;

function formatElapsed(ms: number): string {
  return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
}

function buildFooter(sessionId: string, result: SessionReadResult): string {
  const elapsed = formatElapsed(result.elapsedMs);
  const overflowNote = result.truncated
    ? "\n[Oldest output was dropped: the session buffer overflowed since the last read.]"
    : "";

  if (result.status === "running") {
    return `[running | session: ${sessionId} | ${elapsed} elapsed]${overflowNote}`;
  }

  const signalNote = result.signal ? ` (${result.signal})` : "";
  return `[exit:${result.exitCode ?? 1}${signalNote} | total ${elapsed}]${overflowNote}`;
}

export const createBashSessionTool = (options: {
  sessionManager: ProcessSessionManager;
}) => {
  const { sessionManager } = options;

  return {
    toolDef: {
      description: toolDescription,
      inputSchema,
    },
    display({ sessionId, input, kill }: BashSessionInputSchema) {
      const action = kill ? "kill" : input ? "stdin" : "poll";
      return `${style.cyan(sessionId)} (${action})`;
    },
    async execute(
      { sessionId, input, wait, kill }: BashSessionInputSchema,
      { abortSignal }: ToolExecutionOptions,
    ): Promise<string> {
      if (abortSignal?.aborted) {
        throw new Error("Session interaction aborted");
      }

      if (kill) {
        sessionManager.kill(sessionId);
      } else if (input) {
        sessionManager.write(sessionId, input);
      }

      // After a kill, wait only briefly for the exit to be observed.
      const waitMs = kill ? 2000 : Math.min(wait ?? DEFAULT_WAIT, MAX_WAIT);

      const result = await sessionManager.read(sessionId, { waitMs });

      return truncateOutput(result.output, buildFooter(sessionId, result));
    },
  };
};
