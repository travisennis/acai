import assert from "node:assert/strict";
import { after, describe, it } from "node:test";
import { config } from "../../source/config/index.ts";
import { ProcessSessionManager } from "../../source/execution/process-session.ts";
import { createBashTool } from "../../source/tools/bash.ts";
import { createBashSessionTool } from "../../source/tools/bash-session.ts";

await config.getConfig();

const baseDir = process.cwd();
const sessionManager = new ProcessSessionManager();
const sessionTool = createBashSessionTool({ sessionManager });
const bashTool = await createBashTool({
  workspace: { primaryDir: baseDir, allowedDirs: [baseDir] },
  sessionManager,
});

after(() => {
  sessionManager.killAll();
});

async function startSession(command: string, timeout: number) {
  const result = await bashTool.execute(
    { command, cwd: baseDir, timeout },
    { toolCallId: "t1", messages: [] },
  );
  const match = result.match(/session: (bash_[0-9a-f]+)/);
  assert.ok(match?.[1], `expected a session id in Bash result: ${result}`);
  return { sessionId: match[1], initialOutput: result };
}

async function interact(
  input: Partial<Parameters<typeof sessionTool.execute>[0]> & {
    sessionId: string;
  },
) {
  return sessionTool.execute(
    { input: null, wait: null, ...input },
    { toolCallId: "t2", messages: [] },
  );
}

describe("BashSession polling", () => {
  it("returns incremental output across polls", async () => {
    const { sessionId, initialOutput } = await startSession(
      "echo first; sleep 0.5; echo second; sleep 30",
      200,
    );
    assert.ok(initialOutput.includes("first"));

    const poll = await interact({ sessionId, wait: 5000 });
    assert.ok(poll.includes("second"));
    assert.ok(!poll.includes("first"), "output is not repeated");
    assert.ok(poll.includes(`[running | session: ${sessionId}`));

    await interact({ sessionId, kill: true });
  });

  it("resolves early on exit and reports the exit code", async () => {
    const { sessionId } = await startSession("sleep 0.4; exit 5", 100);

    const before = Date.now();
    let result = await interact({ sessionId, wait: 30_000 });
    assert.ok(Date.now() - before < 10_000, "poll should not wait full wait");

    if (!result.includes("[exit:")) {
      result = await interact({ sessionId, wait: 5000 });
    }
    assert.ok(result.includes("[exit:5"), `expected exit code 5: ${result}`);
  });
});

describe("BashSession stdin", () => {
  it("drives an interactive command to completion", async () => {
    const { sessionId } = await startSession("head -n 1", 200);

    let result = await interact({
      sessionId,
      input: "typed-into-stdin\n",
      wait: 5000,
    });
    assert.ok(result.includes("typed-into-stdin"));

    if (!result.includes("[exit:")) {
      result = await interact({ sessionId, wait: 5000 });
    }
    assert.ok(result.includes("[exit:0"), `expected exit 0: ${result}`);
  });
});

describe("BashSession kill", () => {
  it("terminates the session and reports the signal", async () => {
    const { sessionId } = await startSession("sleep 30", 100);

    const result = await interact({ sessionId, kill: true });
    assert.ok(
      result.includes("[exit:") && result.includes("SIGTERM"),
      `expected a signal exit footer: ${result}`,
    );
  });
});

describe("BashSession errors", () => {
  it("unknown session id produces a helpful error", async () => {
    await assert.rejects(
      () => interact({ sessionId: "bash_nope" }),
      /Unknown session: bash_nope/,
    );
  });
});
