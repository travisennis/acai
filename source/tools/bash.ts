import { randomBytes } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { z } from "zod";
import { ttySizeEnv, validateCommandSafety } from "../execution/index.ts";
import {
  filteredProcessEnv,
  ProcessSessionManager,
  setupSessionCleanup,
} from "../execution/process-session.ts";
import type { WorkspaceContext } from "../index.ts";
import style from "../terminal/style.ts";
import { resolveCwd, validatePaths } from "../utils/bash.ts";
import {
  formatBinaryMessage,
  isBinaryOutput,
  saveBinaryOutput,
} from "../utils/binary-output.ts";
import {
  detectDestructiveCommand,
  formatBlockedCommandMessage,
} from "../utils/command-protection.ts";
import { expandEnvVars } from "../utils/env-expand.ts";
import { logger } from "../utils/logger.ts";
import { convertNullString } from "../utils/zod.ts";
import type { ToolExecutionOptions } from "./types.ts";

/**
 * Detects git commit commands with multi-line -m messages that will fail in shell.
 * Writes the message to a temp file and returns an error with the file path.
 * Returns null if the command is safe.
 */
function detectMultilineGitCommit(command: string): string | null {
  const trimmed = command.trim();

  // Check if it's a git commit command
  if (!trimmed.startsWith("git commit")) {
    return null;
  }

  // Look for -m or -am flags with a message containing newlines
  // Match patterns like: git commit -m "message\nwith\nnewlines"
  // or: git commit -am "message\nwith\nnewlines"
  // Using [\s\S] instead of [^] to match any character including newlines
  const messageMatch = trimmed.match(/-am?\s+["']([\s\S]*?)["']/);
  if (!messageMatch) {
    return null;
  }

  const message = messageMatch[1];
  if (message.includes("\n")) {
    const randomId = randomBytes(4).toString("hex");
    const commitMsgPath = `/tmp/acai/commit-msg-${randomId}.txt`;
    try {
      mkdirSync(dirname(commitMsgPath), { recursive: true });
      writeFileSync(commitMsgPath, message, "utf-8");
    } catch (error) {
      logger.error(error, "Failed to write commit message to temp file");
    }
    return `Multi-line commit messages with -m flag cause shell parsing errors. The commit message has been written to:
  ${commitMsgPath}
Use: git commit -F ${commitMsgPath}`;
  }

  return null;
}

export const BashTool = {
  name: "Bash" as const,
};

const simpleDescription =
  "Run terminal commands. When you need to run multiple independent commands (e.g. `git status`, `git diff`, `git log`; or several `rg`/`grep` searches with different patterns), ALWAYS issue multiple Bash tool calls in the same assistant message rather than running one, waiting for the result, then running the next. The runtime executes parallel tool calls concurrently, so batching independent commands is several times faster than serial calls. Only sequence commands when one truly depends on the output of another. A command that outlives its timeout is not killed: the call returns a session id and the command keeps running — use the BashSession tool to poll for more output, send stdin, or kill it. Set background: true to get a session id immediately for commands meant to keep running (dev servers, watchers).";

// Default yield window in milliseconds: how long to wait for completion
// before the command yields a session and keeps running
const DEFAULT_TIMEOUT = 1.5 * 60 * 1000; // 1.5 minutes

// Maximum output size in bytes (50KB) to prevent context window exhaustion
const MAX_OUTPUT_SIZE = 50 * 1024;

/**
 * Truncates output if it exceeds MAX_OUTPUT_SIZE and adds a clear message.
 * This prevents extremely large outputs from exhausting the context window.
 * The footer is always appended at the end, even when output is truncated.
 */
export function truncateOutput(output: string, footer: string): string {
  if (output.length === 0) {
    return footer;
  }

  if (output.length <= MAX_OUTPUT_SIZE) {
    return `${output}\n${footer}`;
  }

  const truncatedLength = MAX_OUTPUT_SIZE;
  const originalLength = output.length;
  const truncated = output.slice(0, truncatedLength);

  return `${truncated}\n\n[OUTPUT TRUNCATED: ${originalLength.toLocaleString()} characters total, showing first ${truncatedLength.toLocaleString()} characters. The output was too large and was truncated to prevent context window exhaustion. Consider using commands that produce smaller output (e.g., head, tail with line limits, or redirecting to a file).]\n${footer}`;
}

const inputSchema = z.object({
  command: z.string().describe("Full CLI command to execute."),
  cwd: z
    .preprocess((val) => convertNullString(val), z.string().nullable())
    .describe(
      "Optional working directory. Commands execute in the project root by default. Only specify if you need a different directory. Must be within allowed directories.",
    ),
  timeout: z
    .preprocess((val) => convertNullString(val), z.coerce.number().nullable())
    .describe(
      `Yield window in milliseconds. Required but nullable. If null, the default value is ${DEFAULT_TIMEOUT}ms. A command that outlives this window is NOT killed: the call returns a session id and the command keeps running; use the BashSession tool to poll it, send stdin, or kill it.`,
    ),
  background: z
    .boolean()
    .optional()
    .describe(
      "Run command in background. If true, returns a session id immediately without waiting; retrieve output later with the BashSession tool.",
    ),
});

type BashInputSchema = z.infer<typeof inputSchema>;

export const createBashTool = async (options: {
  workspace: WorkspaceContext;
  env?: Record<string, string>;
  sessionManager?: ProcessSessionManager;
}) => {
  const { primaryDir, allowedDirs } = options.workspace;
  const configEnv = options.env ? expandEnvVars(options.env) : {};
  const sessionManager = options.sessionManager ?? new ProcessSessionManager();
  setupSessionCleanup(sessionManager);
  const baseEnv: Record<string, string> = {
    ...filteredProcessEnv(),
    // biome-ignore lint/style/useNamingConvention: environment variable.
    NODE_ENV: "production",
    ...configEnv,
  };
  const allowedDirectories = allowedDirs ?? [primaryDir];

  function validateCommand(
    command: string,
    allowedDirs: string[],
    cwd: string,
  ): void {
    const pathValidation = validatePaths(command, allowedDirs, cwd);
    if (!pathValidation.isValid) {
      throw new Error(pathValidation.error ?? "Unknown error.");
    }

    const multilineError = detectMultilineGitCommit(command);
    if (multilineError) {
      throw new Error(multilineError);
    }

    const destructiveCheck = detectDestructiveCommand(command);
    if (destructiveCheck.blocked) {
      throw new Error(formatBlockedCommandMessage(destructiveCheck));
    }

    validateCommandSafety(command);
  }

  function processCommand(cmd: string, isBackground: boolean): string {
    const stripped = stripTrailingAmpersand(cmd, isBackground);
    return fixRgCommand(stripped);
  }

  async function runCommand(
    cmd: string,
    cwd: string,
    yieldMs: number,
    signal: AbortSignal | undefined,
  ): Promise<string> {
    const result = await sessionManager.start(cmd, {
      cwd,
      env: { ...baseEnv, ...ttySizeEnv() },
      yieldMs,
      abortSignal: signal,
    });

    if (result.type === "running") {
      logger.debug(
        { sessionId: result.sessionId, command: cmd },
        "Command yielded a session",
      );
      const overflowNote = result.truncated
        ? "\n[Oldest output was dropped: the session buffer overflowed before yielding.]"
        : "";
      const footer = `[still running | session: ${result.sessionId}]${overflowNote}\nThe command was NOT killed; it is still running. Use the BashSession tool with sessionId "${result.sessionId}" to poll for new output, send stdin, or kill it.`;
      return truncateOutput(result.output, footer);
    }

    const timeStr =
      result.duration < 1000
        ? `${result.duration}ms`
        : `${(result.duration / 1000).toFixed(1)}s`;
    const metadataFooter = `[exit:${result.exitCode} | ${timeStr}]`;

    // Check for binary output and handle specially
    if (result.exitCode === 0 && isBinaryOutput(result.output)) {
      const saveResult = saveBinaryOutput(result.output);
      const binaryMessage = formatBinaryMessage(saveResult);
      return `${binaryMessage}\n${metadataFooter}`;
    }

    return truncateOutput(result.output, metadataFooter);
  }

  return {
    toolDef: {
      description: simpleDescription,
      inputSchema,
    },
    display({ command }: BashInputSchema) {
      return `${style.cyan(command)}`;
    },
    async execute(
      { command, cwd, timeout, background }: BashInputSchema,
      { abortSignal }: ToolExecutionOptions,
    ): Promise<string> {
      if (abortSignal?.aborted) {
        throw new Error("Command execution aborted");
      }

      const safeCwd = cwd === "null" ? null : cwd;
      const resolvedCwd = resolveCwd(safeCwd, primaryDir, allowedDirectories);
      const safeTimeout = timeout ?? DEFAULT_TIMEOUT;

      validateCommand(command, allowedDirectories, resolvedCwd);

      if (abortSignal?.aborted) {
        throw new Error("Command execution aborted before running the command");
      }

      const processedCommand = processCommand(command, background ?? false);

      // background yields immediately; otherwise wait out the yield window
      const yieldMs = background ? 0 : safeTimeout;

      return runCommand(processedCommand, resolvedCwd, yieldMs, abortSignal);
    },
  };
};

/**
 * Fix rg commands that don't have an explicit path
 * rg hangs when stdin is a socket and no path is given
 * See: https://github.com/BurntSushi/ripgrep/discussions/2047
 */
function fixRgCommand(command: string): string {
  const trimmed = command.trim();

  // Check if command starts with rg
  if (!trimmed.startsWith("rg ") && !trimmed.startsWith("rg\\")) {
    return command;
  }

  // Check if command already has stdin redirection or piping
  // Don't modify commands like: cat file.txt | rg pattern
  // or rg pattern < input.txt
  if (trimmed.includes("|") || trimmed.includes("<") || trimmed.includes(">")) {
    return command;
  }

  // Simple heuristic: if last token starts with -, add .
  // This handles cases like: rg -l pattern --type ts --type js
  const tokens = trimmed.split(/\s+/);
  const lastToken = tokens[tokens.length - 1];

  if (lastToken?.startsWith("-")) {
    // Command ends with an option, need to add path
    logger.debug(`Adding '.' to rg command: ${command}`);
    return `${command} .`;
  }

  // Last token doesn't start with -, could be a path or pattern
  if (lastToken) {
    // If it's ., ./, /, or contains /, assume it's a path
    if (
      lastToken === "." ||
      lastToken.startsWith("./") ||
      lastToken.startsWith("/") ||
      lastToken.includes("/") ||
      lastToken === ".."
    ) {
      // Already has a path
      return command;
    }
    // Check if it's a simple pattern (no special chars that would make it a path)
    // If it's just alphanumeric with maybe some regex chars, it's probably a pattern
    // Common pattern chars: ., *, +, ?, [, ], ^, $, (, ), |, \\
    // But we want to be conservative - if it looks like a filename without path, add .
    if (
      !lastToken.includes("/") &&
      !lastToken.includes("*") &&
      !lastToken.includes(".")
    ) {
      logger.debug(`Adding '.' to rg command: ${command}`);
      return `${command} .`;
    }
    // Complex case with * or other chars, could be a glob pattern
    // Default to adding . to be safe
  }

  // No last token or complex case, add . to be safe
  logger.debug(`Adding '.' to rg command: ${command}`);
  return `${command} .`;
}

/**
 * Strips trailing '&' from command and logs a warning.
 * Returns the processed command.
 */
function stripTrailingAmpersand(
  command: string,
  isBackground: boolean,
): string {
  let processedCommand = command.trim();
  if (processedCommand.endsWith("&")) {
    logger.warn(
      `Stripping '&' from command since background=${String(isBackground)}: ${command}`,
    );
    processedCommand = processedCommand.slice(0, -1).trim();
  }
  return processedCommand;
}
