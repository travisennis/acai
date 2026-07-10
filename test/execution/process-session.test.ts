import assert from "node:assert/strict";
import { afterEach, describe, test } from "node:test";
import { ProcessSessionManager } from "../../source/execution/process-session.ts";

function createManager(
  options: ConstructorParameters<typeof ProcessSessionManager>[0] = {},
) {
  const manager = new ProcessSessionManager(options);
  managers.push(manager);
  return manager;
}

const managers: ProcessSessionManager[] = [];

afterEach(() => {
  for (const manager of managers) {
    manager.killAll();
  }
  managers.length = 0;
});

describe("start: completion within the yield window", () => {
  test("returns completed result with output and exit code 0", async () => {
    const manager = createManager();
    const result = await manager.start("echo -n hello", { yieldMs: 5000 });

    assert.strictEqual(result.type, "completed");
    assert.strictEqual(result.output, "hello");
    assert.strictEqual(result.exitCode, 0);
    assert.strictEqual(manager.list().length, 0);
  });

  test("captures non-zero exit codes", async () => {
    const manager = createManager();
    const result = await manager.start("echo failing; exit 3", {
      yieldMs: 5000,
    });

    assert.strictEqual(result.type, "completed");
    assert.ok(result.output.includes("failing"));
    assert.strictEqual(result.exitCode, 3);
  });

  test("interleaves stdout and stderr", async () => {
    const manager = createManager();
    const result = await manager.start("echo out; echo err >&2", {
      yieldMs: 5000,
    });

    assert.strictEqual(result.type, "completed");
    assert.ok(result.output.includes("out"));
    assert.ok(result.output.includes("err"));
  });
});

describe("start: yielding a session", () => {
  test("slow command yields a session with partial output and keeps running", async () => {
    const manager = createManager();
    const result = await manager.start("echo started; sleep 30; echo done", {
      yieldMs: 500,
    });

    assert.strictEqual(result.type, "running");
    assert.ok(result.sessionId.startsWith("bash_"));
    assert.ok(result.output.includes("started"));

    const sessions = manager.list();
    assert.strictEqual(sessions.length, 1);
    assert.strictEqual(sessions[0]?.status, "running");
  });

  test("yieldMs 0 yields immediately", async () => {
    const manager = createManager();
    const result = await manager.start("sleep 30", { yieldMs: 0 });

    assert.strictEqual(result.type, "running");
  });

  test("exit code is captured when the command finishes after yielding", async () => {
    const manager = createManager();
    const started = await manager.start("sleep 0.3; echo late; exit 7", {
      yieldMs: 50,
    });
    assert.strictEqual(started.type, "running");
    if (started.type !== "running") return;

    const read = await manager.read(started.sessionId, { waitMs: 5000 });
    assert.ok(read.output.includes("late"));

    // Output may arrive before exit is observed; poll once more if needed.
    const final =
      read.status === "exited"
        ? read
        : await manager.read(started.sessionId, { waitMs: 5000 });
    assert.strictEqual(final.status, "exited");
    assert.strictEqual(final.exitCode, 7);
  });
});

describe("read: incremental output", () => {
  test("consecutive reads return disjoint output", async () => {
    const manager = createManager();
    const started = await manager.start(
      "echo first; sleep 0.5; echo second; sleep 30",
      { yieldMs: 100 },
    );
    assert.strictEqual(started.type, "running");
    if (started.type !== "running") return;

    assert.ok(started.output.includes("first"));
    assert.ok(!started.output.includes("second"));

    const read = await manager.read(started.sessionId, { waitMs: 5000 });
    assert.ok(read.output.includes("second"));
    assert.ok(!read.output.includes("first"));
    assert.strictEqual(read.status, "running");
  });

  test("read resolves early on exit even with no new output", async () => {
    const manager = createManager();
    const started = await manager.start("sleep 0.3", { yieldMs: 50 });
    assert.strictEqual(started.type, "running");
    if (started.type !== "running") return;

    const before = Date.now();
    const read = await manager.read(started.sessionId, { waitMs: 30_000 });
    assert.ok(Date.now() - before < 10_000, "read should not wait full waitMs");
    assert.strictEqual(read.status, "exited");
    assert.strictEqual(read.exitCode, 0);
  });
});

describe("output buffer overflow", () => {
  test("drops oldest output and reports truncation", async () => {
    const manager = createManager({ maxPendingBytes: 1024 });
    const result = await manager.start(
      "node -e \"process.stdout.write('x'.repeat(4096) + 'TAIL')\"",
      { yieldMs: 10_000 },
    );

    assert.strictEqual(result.type, "completed");
    assert.ok(result.output.length <= 1024);
    assert.ok(result.output.endsWith("TAIL"), "newest output is kept");
    assert.strictEqual(result.truncated, true);
  });
});

describe("stdin", () => {
  test("drives an interactive command to completion", async () => {
    const manager = createManager();
    const started = await manager.start("head -n 1", { yieldMs: 200 });
    assert.strictEqual(started.type, "running");
    if (started.type !== "running") return;

    manager.write(started.sessionId, "hello-from-stdin\n");

    const read = await manager.read(started.sessionId, { waitMs: 5000 });
    assert.ok(read.output.includes("hello-from-stdin"));

    const final =
      read.status === "exited"
        ? read
        : await manager.read(started.sessionId, { waitMs: 5000 });
    assert.strictEqual(final.status, "exited");
    assert.strictEqual(final.exitCode, 0);
  });

  test("write to a process that closed its stdin does not crash", async () => {
    const manager = createManager();
    const started = await manager.start("sh -c 'exec 0<&-; sleep 30'", {
      yieldMs: 300,
    });
    assert.strictEqual(started.type, "running");
    if (started.type !== "running") return;

    // The pipe is closed on the far side; the EPIPE must be captured as
    // session output instead of crashing the process.
    manager.write(started.sessionId, "hello\n");

    const read = await manager.read(started.sessionId, { waitMs: 5000 });
    assert.ok(
      read.output.includes("stdin write failed"),
      `expected stdin failure note in output: ${read.output}`,
    );
    assert.strictEqual(read.status, "running");
  });

  test("write to an exited session throws", async () => {
    const manager = createManager({ exitedTtlMs: 60_000 });
    const started = await manager.start("sleep 0.1", { yieldMs: 0 });
    assert.strictEqual(started.type, "running");
    if (started.type !== "running") return;

    // Wait for exit without draining/reaping via read.
    await new Promise((resolve) => setTimeout(resolve, 500));

    assert.throws(() => {
      manager.write(started.sessionId, "too late\n");
    }, /already exited/);
  });
});

describe("kill", () => {
  test("terminates a running session and reports the signal", async () => {
    const manager = createManager();
    const started = await manager.start("sleep 30", { yieldMs: 50 });
    assert.strictEqual(started.type, "running");
    if (started.type !== "running") return;

    manager.kill(started.sessionId);

    const read = await manager.read(started.sessionId, { waitMs: 5000 });
    assert.strictEqual(read.status, "exited");
    assert.strictEqual(read.signal, "SIGTERM");
  });

  test("kills the whole process group, including grandchildren", async () => {
    const manager = createManager();
    const started = await manager.start(
      "sh -c 'sleep 30 & echo child-pid $!; wait'",
      { yieldMs: 500 },
    );
    assert.strictEqual(started.type, "running");
    if (started.type !== "running") return;

    const pidMatch = started.output.match(/child-pid (\d+)/);
    assert.ok(pidMatch?.[1], "should have captured grandchild pid");
    const grandchildPid = Number(pidMatch[1]);

    manager.kill(started.sessionId);
    await manager.read(started.sessionId, { waitMs: 5000 });

    // Give the signal a moment to be delivered, then verify the grandchild
    // is gone. process.kill(pid, 0) throws ESRCH for dead processes.
    await new Promise((resolve) => setTimeout(resolve, 200));
    assert.throws(() => {
      process.kill(grandchildPid, 0);
    });
  });
});

describe("session lifecycle", () => {
  test("enforces the concurrent session cap", async () => {
    const manager = createManager({ maxSessions: 2 });
    await manager.start("sleep 30", { yieldMs: 0 });
    await manager.start("sleep 30", { yieldMs: 0 });

    await assert.rejects(async () => {
      await manager.start("sleep 30", { yieldMs: 0 });
    }, /Too many running sessions/);
  });

  test("drained exited sessions are reaped", async () => {
    const manager = createManager();
    const started = await manager.start("sleep 0.2; echo bye", { yieldMs: 50 });
    assert.strictEqual(started.type, "running");
    if (started.type !== "running") return;

    // Drain until exited (streams must close for the reap to happen).
    let status = "running";
    for (let i = 0; i < 20 && status !== "exited"; i++) {
      const read = await manager.read(started.sessionId, { waitMs: 1000 });
      status = read.status;
    }
    assert.strictEqual(status, "exited");

    await assert.rejects(async () => {
      await manager.read(started.sessionId, { waitMs: 100 });
    }, /Unknown session/);
  });

  test("unknown session id produces a helpful error", async () => {
    const manager = createManager();
    await manager.start("sleep 30", { yieldMs: 0 });

    await assert.rejects(async () => {
      await manager.read("bash_nope", { waitMs: 100 });
    }, /Unknown session: bash_nope/);
  });

  test("expired exited sessions are swept by TTL", async () => {
    const manager = createManager({ exitedTtlMs: 100 });
    const started = await manager.start("echo gone", { yieldMs: 0 });
    assert.strictEqual(started.type, "running");
    if (started.type !== "running") return;

    // Let it exit and outlive the TTL without draining it. Timing varies
    // under load, so poll until the sweep happens instead of sleeping once.
    const deadline = Date.now() + 10_000;
    let swept = false;
    while (!swept && Date.now() < deadline) {
      await new Promise((resolve) => setTimeout(resolve, 200));
      swept = !manager.list().some((s) => s.sessionId === started.sessionId);
    }
    assert.ok(swept, "session should have been swept by TTL");

    await assert.rejects(async () => {
      await manager.read(started.sessionId, { waitMs: 100 });
    }, /Unknown session/);
  });
});

describe("abort during the yield window", () => {
  test("kills the process and throws", async () => {
    const manager = createManager();
    const controller = new AbortController();
    setTimeout(() => controller.abort(), 200);

    await assert.rejects(async () => {
      await manager.start("sleep 30", {
        yieldMs: 10_000,
        abortSignal: controller.signal,
      });
    }, /aborted/);

    assert.strictEqual(manager.list().length, 0);
  });

  test("already-aborted signal throws before spawning", async () => {
    const manager = createManager();
    const controller = new AbortController();
    controller.abort();

    await assert.rejects(async () => {
      await manager.start("echo hi", {
        yieldMs: 1000,
        abortSignal: controller.signal,
      });
    }, /aborted/);
  });
});
