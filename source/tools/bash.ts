import { randomBytes } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { z } from "zod";
import { initExecutionEnvironment } from "../execution/index.ts";
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
  "Run terminal commands. When you need to run multiple independent commands (e.g. `git status`, `git diff`, `git log`; or several `rg`/`grep` searches with different patterns), ALWAYS issue multiple Bash tool calls in the same assistant message rather than running one, waiting for the result, then running the next. The runtime executes parallel tool calls concurrently, so batching independent commands is several times faster than serial calls. Only sequence commands when one truly depends on the output of another.";

// Command execution timeout in milliseconds
const DEFAULT_TIMEOUT = 1.5 * 60 * 1000; // 1.5 minutes

// Maximum output size in bytes (50KB) to prevent context window exhaustion
const MAX_OUTPUT_SIZE = 50 * 1024;

/**
 * Truncates output if it exceeds MAX_OUTPUT_SIZE and adds a clear message.
 * This prevents extremely large outputs from exhausting the context window.
 * The footer is always appended at the end, even when output is truncated.
 */
function truncateOutput(output: string, footer: string): string {
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
      `Command execution timeout in milliseconds. Required but nullable. If null, the default value is ${DEFAULT_TIMEOUT}ms`,
    ),
  background: z
    .boolean()
    .optional()
    .describe(
      "Run command in background. If true, command will run until program exit.",
    ),
});

type BashInputSchema = z.infer<typeof inputSchema>;

export const createBashTool = async (options: {
  workspace: WorkspaceContext;
  env?: Record<string, string>;
}) => {
  const { primaryDir, allowedDirs } = options.workspace;
  const configEnv = options.env ? expandEnvVars(options.env) : {};
  const execEnv = await initExecutionEnvironment({
    execution: {
      env: {
        ...configEnv,
      },
    },
  });
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
  }

  function processCommand(cmd: string, isBackground: boolean): string {
    const stripped = stripTrailingAmpersand(cmd, isBackground);
    return fixRgCommand(stripped);
  }

  function executeBackground(
    cmd: string,
    cwd: string,
    signal: AbortSignal | undefined,
  ): string {
    const proc = execEnv.executeCommandInBackground(cmd, {
      cwd,
      abortSignal: signal,
      onOutput: (output: string) => {
        logger.debug({ output }, "Background command output:");
      },
      onError: (error: string) => {
        logger.debug({ error }, "Background command error:");
      },
      onExit: (code: number | null) => {
        logger.debug(`Background command exited with code ${code}`);
      },
    });
    return `Background process started with PID: ${proc.pid}`;
  }

  async function executeSync(
    cmd: string,
    cwd: string,
    timeout: number,
    signal: AbortSignal | undefined,
  ): Promise<string> {
    const startTime = Date.now();
    const { output, exitCode, error } = await execEnv.executeCommand(cmd, {
      cwd,
      timeout,
      abortSignal: signal,
      preserveOutputOnError: true,
      captureStderr: true,
      throwOnError: false,
    });
    const elapsedMs = Date.now() - startTime;
    const timeStr =
      elapsedMs < 1000 ? `${elapsedMs}ms` : `${(elapsedMs / 1000).toFixed(1)}s`;

    const metadataFooter = `[exit:${exitCode} | ${timeStr}]`;

    if (exitCode !== 0) {
      const execError = error as
        | (Error & { killed?: boolean; signal?: string | null })
        | undefined;
      const wasKilled = execError?.killed === true || execError?.signal != null;

      if (wasKilled) {
        const timeoutOutput =
          output || `Command timed out after ${timeout.toLocaleString()}ms`;
        return truncateOutput(timeoutOutput, metadataFooter);
      }

      return truncateOutput(output, metadataFooter);
    }

    // Check for binary output and handle specially
    if (isBinaryOutput(output)) {
      const saveResult = saveBinaryOutput(output);
      const binaryMessage = formatBinaryMessage(saveResult);
      return `${binaryMessage}\n${metadataFooter}`;
    }

    return truncateOutput(output, metadataFooter);
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

      if (background) {
        return executeBackground(processedCommand, resolvedCwd, abortSignal);
      }

      return executeSync(
        processedCommand,
        resolvedCwd,
        safeTimeout,
        abortSignal,
      );
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
