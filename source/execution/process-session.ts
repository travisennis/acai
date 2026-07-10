/**
 * Process Session Manager
 *
 * Runs shell commands with a yield window instead of a hard timeout-kill.
 * Commands that finish within the window return a completed result; commands
 * that outlive it keep running as a session that can be polled, fed stdin,
 * or killed later.
 */
import { type ChildProcess, spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import { logger } from "../utils/logger.ts";
import { getShell, ttySizeEnv } from "./index.ts";

/** Maximum buffered-but-unread output per session */
const DEFAULT_MAX_PENDING_BYTES = 1024 * 1024;

/** Maximum number of concurrently running sessions */
const DEFAULT_MAX_SESSIONS = 16;

/** How long an exited session is kept before it is swept, even if undrained */
const DEFAULT_EXITED_TTL_MS = 10 * 60 * 1000;

interface ProcessSession {
  id: string;
  command: string;
  cwd: string;
  startTime: number;
  child: ChildProcess;
  /** Output that has arrived but not yet been delivered to a caller */
  pending: string;
  /** True when pending overflowed and oldest output was dropped since last read */
  truncatedSinceRead: boolean;
  status: "running" | "exited";
  exitCode: number | null;
  exitSignal: NodeJS.Signals | null;
  exitTime: number | null;
  /** All stdio streams have closed; no more output can arrive */
  streamsClosed: boolean;
  /** Resolvers waiting on new output or exit */
  activityWaiters: Array<() => void>;
}

export interface SessionStartOptions {
  cwd?: string;
  env?: Record<string, string>;
  shell?: string;
  /** How long to wait for completion before yielding a session. 0 yields immediately. */
  yieldMs: number;
  abortSignal?: AbortSignal;
}

export type SessionStartResult =
  | {
      type: "completed";
      output: string;
      truncated: boolean;
      exitCode: number;
      signal: NodeJS.Signals | null;
      duration: number;
    }
  | {
      type: "running";
      sessionId: string;
      output: string;
      truncated: boolean;
    };

export interface SessionReadResult {
  output: string;
  truncated: boolean;
  status: "running" | "exited";
  exitCode: number | null;
  signal: NodeJS.Signals | null;
  /** Milliseconds since the command started */
  elapsedMs: number;
}

export interface SessionInfo {
  sessionId: string;
  command: string;
  status: "running" | "exited";
  startTime: Date;
  elapsedMs: number;
}

export interface ProcessSessionManagerOptions {
  maxPendingBytes?: number;
  maxSessions?: number;
  exitedTtlMs?: number;
}

export class ProcessSessionManager {
  private sessions: Map<string, ProcessSession> = new Map();
  private maxPendingBytes: number;
  private maxSessions: number;
  private exitedTtlMs: number;

  constructor(options: ProcessSessionManagerOptions = {}) {
    this.maxPendingBytes = options.maxPendingBytes ?? DEFAULT_MAX_PENDING_BYTES;
    this.maxSessions = options.maxSessions ?? DEFAULT_MAX_SESSIONS;
    this.exitedTtlMs = options.exitedTtlMs ?? DEFAULT_EXITED_TTL_MS;
  }

  /**
   * Start a command. Resolves with a completed result if the command exits
   * within the yield window, otherwise with a running session handle.
   */
  async start(
    command: string,
    options: SessionStartOptions,
  ): Promise<SessionStartResult> {
    this.sweepExpired();

    const running = [...this.sessions.values()].filter(
      (s) => s.status === "running",
    );
    if (running.length >= this.maxSessions) {
      throw new Error(
        `Too many running sessions (${running.length}/${this.maxSessions}). Kill one first. Active sessions:\n${this.describeSessions()}`,
      );
    }

    if (options.abortSignal?.aborted) {
      throw new Error("Command execution aborted");
    }

    const cwd = options.cwd ?? process.cwd();
    const env = options.env ?? {
      ...filteredProcessEnv(),
      ...ttySizeEnv(),
    };
    const shell = options.shell ?? getShell();

    const child = spawn(command, [], {
      cwd,
      env,
      shell,
      detached: true,
      stdio: ["pipe", "pipe", "pipe"],
    });

    const session: ProcessSession = {
      id: `bash_${randomBytes(3).toString("hex")}`,
      command,
      cwd,
      startTime: Date.now(),
      child,
      pending: "",
      truncatedSinceRead: false,
      status: "running",
      exitCode: null,
      exitSignal: null,
      exitTime: null,
      streamsClosed: false,
      activityWaiters: [],
    };
    this.sessions.set(session.id, session);
    this.wireChildEvents(session);

    const outcome = await this.waitForExit(
      session,
      options.yieldMs,
      options.abortSignal,
    );

    if (outcome === "aborted") {
      this.killGroup(session, "SIGTERM");
      this.sessions.delete(session.id);
      throw new Error("Command execution aborted");
    }

    if (outcome === "exited") {
      const result: SessionStartResult = {
        type: "completed",
        output: this.consumePending(session),
        truncated: session.truncatedSinceRead,
        exitCode: session.exitCode ?? (session.exitSignal ? 1 : 0),
        signal: session.exitSignal,
        duration: (session.exitTime ?? Date.now()) - session.startTime,
      };
      this.sessions.delete(session.id);
      return result;
    }

    // Still running: yield a session handle with output so far.
    const output = this.consumePending(session);
    const truncated = session.truncatedSinceRead;
    session.truncatedSinceRead = false;
    return {
      type: "running",
      sessionId: session.id,
      output,
      truncated,
    };
  }

  /**
   * Read new output from a session, waiting up to waitMs for output or exit.
   * Resolves early as soon as there is anything to report.
   */
  async read(
    sessionId: string,
    options: { waitMs: number },
  ): Promise<SessionReadResult> {
    const session = this.getSession(sessionId);

    if (session.pending.length === 0 && session.status === "running") {
      await this.waitForActivity(session, options.waitMs);
    }

    const output = this.consumePending(session);
    const truncated = session.truncatedSinceRead;
    session.truncatedSinceRead = false;

    const result: SessionReadResult = {
      output,
      truncated,
      status: session.status,
      exitCode: session.exitCode,
      signal: session.exitSignal,
      elapsedMs: (session.exitTime ?? Date.now()) - session.startTime,
    };

    // Reap once the session has exited, its streams are closed, and every
    // byte of output has been delivered.
    if (
      session.status === "exited" &&
      session.streamsClosed &&
      session.pending.length === 0
    ) {
      this.sessions.delete(session.id);
    }

    return result;
  }

  /**
   * Write raw input to a running session's stdin.
   */
  write(sessionId: string, input: string): void {
    const session = this.getSession(sessionId);

    if (session.status === "exited") {
      throw new Error(
        `Session ${sessionId} has already exited (exit code ${session.exitCode}); cannot write to stdin.`,
      );
    }
    if (!session.child.stdin || session.child.stdin.destroyed) {
      throw new Error(`Session ${sessionId} has no writable stdin.`);
    }

    session.child.stdin.write(input);
  }

  /**
   * Kill a session's process group.
   */
  kill(sessionId: string, signal: NodeJS.Signals = "SIGTERM"): void {
    const session = this.getSession(sessionId);
    this.killGroup(session, signal);
  }

  /**
   * List active (running and undrained exited) sessions.
   */
  list(): SessionInfo[] {
    this.sweepExpired();
    return [...this.sessions.values()].map((session) => ({
      sessionId: session.id,
      command:
        session.command.length > 80
          ? `${session.command.slice(0, 77)}...`
          : session.command,
      status: session.status,
      startTime: new Date(session.startTime),
      elapsedMs: (session.exitTime ?? Date.now()) - session.startTime,
    }));
  }

  /**
   * Kill every running session. Called on process shutdown.
   */
  killAll(): void {
    for (const session of this.sessions.values()) {
      if (session.status === "running") {
        try {
          this.killGroup(session, "SIGTERM");
        } catch (error) {
          logger.warn(error, `Failed to kill session ${session.id}`);
        }
      }
    }
    this.sessions.clear();
  }

  private getSession(sessionId: string): ProcessSession {
    this.sweepExpired();
    const session = this.sessions.get(sessionId);
    if (!session) {
      const active = this.describeSessions();
      throw new Error(
        `Unknown session: ${sessionId}. ${
          active
            ? `Active sessions:\n${active}`
            : "There are no active sessions."
        }`,
      );
    }
    return session;
  }

  private describeSessions(): string {
    return this.list()
      .map(
        (s) =>
          `  ${s.sessionId} [${s.status}] ${Math.round(s.elapsedMs / 1000)}s: ${s.command}`,
      )
      .join("\n");
  }

  private wireChildEvents(session: ProcessSession): void {
    const append = (data: Buffer) => {
      this.appendOutput(session, data.toString("utf8"));
    };
    session.child.stdout?.on("data", append);
    session.child.stderr?.on("data", append);

    // A write to a closed stdin pipe (EPIPE) emits an async stream error;
    // without a listener it would crash the whole process.
    session.child.stdin?.on("error", (error: Error) => {
      logger.debug(error, `Session ${session.id} stdin error`);
      this.appendOutput(session, `[stdin write failed: ${error.message}]\n`);
    });

    session.child.on("error", (error: Error) => {
      this.appendOutput(session, `Failed to run command: ${error.message}\n`);
      if (session.status === "running") {
        session.status = "exited";
        session.exitCode = 127;
        session.exitTime = Date.now();
      }
      this.notifyActivity(session);
    });

    session.child.on("exit", (code, signal) => {
      session.status = "exited";
      session.exitCode = code;
      session.exitSignal = signal;
      session.exitTime = Date.now();
      this.notifyActivity(session);
    });

    session.child.on("close", () => {
      session.streamsClosed = true;
      this.notifyActivity(session);
    });
  }

  private appendOutput(session: ProcessSession, chunk: string): void {
    session.pending += chunk;
    if (session.pending.length > this.maxPendingBytes) {
      session.pending = session.pending.slice(
        session.pending.length - this.maxPendingBytes,
      );
      session.truncatedSinceRead = true;
    }
    this.notifyActivity(session);
  }

  private notifyActivity(session: ProcessSession): void {
    const waiters = session.activityWaiters;
    session.activityWaiters = [];
    for (const waiter of waiters) {
      waiter();
    }
  }

  private consumePending(session: ProcessSession): string {
    const output = session.pending;
    session.pending = "";
    return output;
  }

  /**
   * Wait until the session exits, the window elapses, or the signal aborts.
   */
  private waitForExit(
    session: ProcessSession,
    windowMs: number,
    abortSignal?: AbortSignal,
  ): Promise<"exited" | "window-elapsed" | "aborted"> {
    if (session.status === "exited") {
      return Promise.resolve("exited");
    }
    if (windowMs <= 0) {
      return Promise.resolve("window-elapsed");
    }

    return new Promise((resolve) => {
      let timer: NodeJS.Timeout | undefined;
      let abortHandler: (() => void) | undefined;

      const settle = (outcome: "exited" | "window-elapsed" | "aborted") => {
        if (timer) clearTimeout(timer);
        if (abortHandler) {
          abortSignal?.removeEventListener("abort", abortHandler);
        }
        resolve(outcome);
      };

      const onActivity = () => {
        if (session.status === "exited") {
          settle("exited");
        } else {
          // Output arrived but the process is still running; keep waiting.
          session.activityWaiters.push(onActivity);
        }
      };
      session.activityWaiters.push(onActivity);

      timer = setTimeout(() => settle("window-elapsed"), windowMs);

      if (abortSignal) {
        abortHandler = () => settle("aborted");
        abortSignal.addEventListener("abort", abortHandler, { once: true });
      }
    });
  }

  /**
   * Wait until the session produces output or exits, up to waitMs.
   */
  private waitForActivity(
    session: ProcessSession,
    waitMs: number,
  ): Promise<void> {
    if (
      waitMs <= 0 ||
      session.pending.length > 0 ||
      session.status === "exited"
    ) {
      return Promise.resolve();
    }

    return new Promise((resolve) => {
      let timer: NodeJS.Timeout | undefined;
      const settle = () => {
        if (timer) clearTimeout(timer);
        resolve();
      };
      session.activityWaiters.push(settle);
      timer = setTimeout(settle, waitMs);
    });
  }

  private killGroup(session: ProcessSession, signal: NodeJS.Signals): void {
    const pid = session.child.pid;
    if (pid === undefined || session.status === "exited") {
      return;
    }
    try {
      // detached: true makes the child a process group leader, so a negative
      // pid signals the whole group, including grandchildren.
      process.kill(-pid, signal);
    } catch {
      session.child.kill(signal);
    }
  }

  private sweepExpired(): void {
    const now = Date.now();
    for (const [id, session] of this.sessions) {
      if (
        session.status === "exited" &&
        session.exitTime !== null &&
        now - session.exitTime > this.exitedTtlMs
      ) {
        this.sessions.delete(id);
      }
    }
  }
}

// Track registered managers for cleanup
const registeredForCleanup = new WeakSet<ProcessSessionManager>();

/**
 * Kill all sessions when the acai process exits.
 */
export function setupSessionCleanup(manager: ProcessSessionManager): void {
  if (registeredForCleanup.has(manager)) {
    return;
  }
  registeredForCleanup.add(manager);

  const cleanup = () => {
    manager.killAll();
  };

  process.on("exit", cleanup);
  process.on("SIGINT", cleanup);
  process.on("SIGTERM", cleanup);
}

export function filteredProcessEnv(): Record<string, string> {
  return Object.fromEntries(
    Object.entries(process.env).filter(
      (entry): entry is [string, string] => entry[1] !== undefined,
    ),
  );
}
