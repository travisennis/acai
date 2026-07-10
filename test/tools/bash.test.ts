import assert from "node:assert/strict";
import { after, describe, it } from "node:test";
import { config } from "../../source/config/index.ts";
import { ProcessSessionManager } from "../../source/execution/process-session.ts";
import { createBashTool } from "../../source/tools/bash.ts";
import { validatePaths } from "../../source/utils/bash.ts";

await config.getConfig();

const baseDir = process.cwd();

describe("bash tool path validation for git message flags", async () => {
  const tool = await createBashTool({
    workspace: { primaryDir: baseDir, allowedDirs: [baseDir] },
  });

  async function run(command: string) {
    return tool.execute(
      { command, cwd: baseDir, timeout: 1000 },
      { toolCallId: "t1", messages: [] },
    );
  }

  it("allows commit messages with /copy", async () => {
    try {
      await run('echo ok && git commit -m "docs: mention /copy"');
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      assert.ok(
        !msg.includes("resolves outside the allowed directories"),
        `Path validation should not block commit message, got: ${msg}`,
      );
    }
  });

  it("allows URLs in messages", async () => {
    try {
      await run('git commit -m "URL https://example.com/p/a/t/h"');
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      assert.ok(
        !msg.includes("resolves outside the allowed directories"),
        `Path validation should not block commit message, got: ${msg}`,
      );
    }
  });

  it("rejects absolute path in git add", async () => {
    await assert.rejects(
      () => run("git add /etc/hosts"),
      /resolves outside the allowed directories/,
    );
  });

  it("rejects commit -F with file outside", async () => {
    await assert.rejects(
      () => run("git commit -F /tmp/message.txt"),
      /resolves outside the allowed directories/,
    );
  });

  it("handles multiple -m flags", async () => {
    try {
      await run('git commit -m "first /copy" -m "second /path"');
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      assert.ok(
        !msg.includes("resolves outside the allowed directories"),
        `Path validation should not block commit message, got: ${msg}`,
      );
    }
  });
});

describe("bash tool allowed paths access", async () => {
  const tool = await createBashTool({
    workspace: { primaryDir: baseDir, allowedDirs: [baseDir, "/tmp"] },
  });

  async function run(command: string, timeout = 5000) {
    return tool.execute(
      { command, cwd: baseDir, timeout },
      { toolCallId: "t1", messages: [] },
    );
  }

  it("allows access to configured allowed paths", async () => {
    const res = await run("ls /tmp");
    assert.ok(!res.includes("resolves outside the allowed directories"));
    assert.ok(res.length >= 0);
  });

  it("rejects access to other files outside project directory", async () => {
    await assert.rejects(
      () => run("cat /etc/hosts"),
      /resolves outside the allowed directories/,
    );
  });
});

describe("bash tool abort signal handling", async () => {
  it("aborts execution on signal", async () => {
    const ac = new AbortController();
    const tool = await createBashTool({
      workspace: { primaryDir: baseDir, allowedDirs: [baseDir] },
    });
    ac.abort();
    const { execute } = tool;
    const result = execute(
      { command: "sleep 10", cwd: null, timeout: null },
      { toolCallId: "t1", abortSignal: ac.signal, messages: [] },
    );

    await assert.rejects(result, /aborted/);
  });
});

describe("bash tool yield behavior", async () => {
  const sessionManager = new ProcessSessionManager();
  const tool = await createBashTool({
    workspace: { primaryDir: baseDir, allowedDirs: [baseDir] },
    sessionManager,
  });

  after(() => {
    sessionManager.killAll();
  });

  it("yields a session instead of killing a command that outlives the timeout", async () => {
    const result = await tool.execute(
      { command: "echo early-output; sleep 30", cwd: baseDir, timeout: 300 },
      { toolCallId: "t1", messages: [] },
    );

    assert.ok(result.includes("early-output"), "partial output is returned");
    assert.ok(result.includes("still running"));
    const match = result.match(/session: (bash_[0-9a-f]+)/);
    assert.ok(match?.[1], `expected a session id in result: ${result}`);
    assert.ok(!result.includes("timed out"));

    // The process must not have been killed at the yield boundary.
    const sessions = sessionManager.list();
    assert.strictEqual(sessions.length, 1);
    assert.strictEqual(sessions[0]?.status, "running");
  });

  it("background returns a session immediately with retrievable output", async () => {
    const result = await tool.execute(
      {
        command: "echo bg-output; sleep 30",
        cwd: baseDir,
        timeout: null,
        background: true,
      },
      { toolCallId: "t1", messages: [] },
    );

    const match = result.match(/session: (bash_[0-9a-f]+)/);
    assert.ok(match?.[1], `expected a session id in result: ${result}`);
    const sessionId = match[1];

    // Output that previously vanished into logger.debug is now retrievable.
    const read = await sessionManager.read(sessionId, { waitMs: 5000 });
    assert.ok(read.output.includes("bg-output"));
    assert.strictEqual(read.status, "running");
  });

  it("commands that finish within the window return no session", async () => {
    const before = sessionManager.list().length;
    const result = await tool.execute(
      { command: "echo -n fast", cwd: baseDir, timeout: 5000 },
      { toolCallId: "t1", messages: [] },
    );

    assert.ok(result.startsWith("fast"));
    assert.ok(result.includes("[exit:0"));
    assert.ok(!result.includes("still running"));
    assert.strictEqual(sessionManager.list().length, before);
  });
});

describe("bash tool exit metadata", async () => {
  it("returns only exit metadata for silent non-zero exits", async () => {
    const tool = await createBashTool({
      workspace: { primaryDir: baseDir, allowedDirs: [baseDir] },
    });
    const { execute } = tool;

    const result = await execute(
      { command: "exit 1", cwd: baseDir, timeout: 1000 },
      { toolCallId: "t1", messages: [] },
    );

    assert.ok(
      result.startsWith("[exit:1"),
      `Result should only contain exit metadata, got: ${result}`,
    );
    assert.ok(!result.includes("Command failed"));
  });

  it("returns command output and exit metadata when command fails", async () => {
    const tool = await createBashTool({
      workspace: { primaryDir: baseDir, allowedDirs: [baseDir] },
    });
    const { execute } = tool;

    const result = await execute(
      {
        command:
          "node -e \"console.error('expected stderr'); process.exit(2)\"",
        cwd: baseDir,
        timeout: 1000,
      },
      { toolCallId: "t1", messages: [] },
    );

    assert.ok(result.includes("expected stderr"));
    assert.ok(
      result.includes("[exit:2"),
      `Result should mention exit code, got: ${result}`,
    );
    assert.ok(!result.includes("Command failed"));
  });
});

describe("bash tool system temp directories validation", () => {
  const baseDir = process.cwd();
  // Default allowed dirs include /tmp and /var/folders
  const allowedDirsWithTemp = [baseDir, "/tmp", "/tmp/acai", "/var/folders"];

  it("allows ls /tmp command", () => {
    const result = validatePaths("ls /tmp", allowedDirsWithTemp, baseDir);
    assert.strictEqual(result.isValid, true);
  });

  it("allows cat /tmp/test.txt command", () => {
    const result = validatePaths(
      "cat /tmp/test.txt",
      allowedDirsWithTemp,
      baseDir,
    );
    assert.strictEqual(result.isValid, true);
  });

  it("allows echo > /tmp/test.txt command", () => {
    const result = validatePaths(
      'echo "test" > /tmp/test.txt',
      allowedDirsWithTemp,
      baseDir,
    );
    assert.strictEqual(result.isValid, true);
  });

  it("allows /var/folders paths", () => {
    const result = validatePaths(
      "ls /var/folders/xx",
      allowedDirsWithTemp,
      baseDir,
    );
    assert.strictEqual(result.isValid, true);
  });

  it("allows /var/folders/.../temp paths", () => {
    const result = validatePaths(
      "cat /var/folders/wk/2s01rzs92955clqwrzb3z84m0000gn/T/acai-grep-test/test.txt",
      allowedDirsWithTemp,
      baseDir,
    );
    assert.strictEqual(result.isValid, true);
  });
});

describe("bash tool home directory (~) validation", () => {
  it("rejects ls ~ command", () => {
    const result = validatePaths("ls ~", [baseDir], baseDir);
    assert.strictEqual(result.isValid, false);
    assert.ok(
      result.error?.includes("resolves outside the allowed directories"),
    );
  });

  it("rejects ls ~/Documents command", () => {
    const result = validatePaths("ls ~/Documents", [baseDir], baseDir);
    assert.strictEqual(result.isValid, false);
    assert.ok(
      result.error?.includes("resolves outside the allowed directories"),
    );
  });

  it("rejects echo > ~/test.txt command", () => {
    const result = validatePaths(
      'echo "test" > ~/test.txt',
      [baseDir],
      baseDir,
    );
    assert.strictEqual(result.isValid, false);
    assert.ok(
      result.error?.includes("resolves outside the allowed directories"),
    );
  });

  it("allows commands without ~ paths", () => {
    const result = validatePaths("ls ./source", [baseDir], baseDir);
    assert.strictEqual(result.isValid, true);
  });
});

describe("bash tool path validation edge cases", async () => {
  const tool = await createBashTool({
    workspace: { primaryDir: baseDir, allowedDirs: [baseDir] },
  });

  async function run(command: string) {
    return tool.execute(
      { command, cwd: baseDir, timeout: 5000 },
      { toolCallId: "t1", messages: [] },
    );
  }

  it("allows ls with relative subdirectory", async () => {
    const result = await run("ls source/");
    assert.ok(result.length >= 0);
  });

  it("allows ls with dot path", async () => {
    const result = await run("ls .");
    assert.ok(result.length >= 0);
  });

  it("allows cat of a known project file", async () => {
    const result = await run("cat package.json");
    assert.ok(result.includes("name"));
  });

  it("allows piped commands within project", async () => {
    const result = await run("cat package.json | head -5");
    assert.ok(result.length > 0);
  });

  it("rejects cat of /etc/passwd", async () => {
    await assert.rejects(
      () => run("cat /etc/passwd"),
      /resolves outside the allowed directories/,
    );
  });

  it("rejects parent traversal escaping project", async () => {
    await assert.rejects(
      () => run("cat ../../../etc/shadow"),
      /resolves outside the allowed directories/,
    );
  });

  it("allows git log with flags (no external paths)", async () => {
    const result = await run("git log --oneline -5");
    assert.ok(result.length >= 0);
  });

  it("allows git diff with project-relative path", async () => {
    const result = await run("git diff HEAD -- source/index.ts");
    assert.ok(typeof result === "string");
  });

  it("rejects find targeting /usr", async () => {
    await assert.rejects(
      () => run("find /usr -name '*.ts'"),
      /resolves outside the allowed directories/,
    );
  });

  it("allows wc -l on project file", async () => {
    const result = await run("wc -l package.json");
    assert.ok(result.length > 0);
  });

  it("allows commands with URLs (no false positive)", async () => {
    const result = await run(
      "echo https://github.com/owner/repo/blob/main/file.ts",
    );
    assert.ok(result.includes("https://"));
  });

  it("allows grep within project", async () => {
    const result = await run("grep -r 'validatePaths' source/utils/bash.ts");
    assert.ok(result.length > 0);
  });

  it("rejects grep targeting /var/log", async () => {
    await assert.rejects(
      () => run("grep -r error /var/log/"),
      /resolves outside the allowed directories/,
    );
  });
});

describe("bash tool multiple allowed dirs", async () => {
  const tool = await createBashTool({
    workspace: {
      primaryDir: baseDir,
      allowedDirs: [baseDir, "/tmp", "/var/folders"],
    },
  });

  async function run(command: string) {
    return tool.execute(
      { command, cwd: baseDir, timeout: 5000 },
      { toolCallId: "t1", messages: [] },
    );
  }

  it("allows ls /tmp with multiple allowed dirs", async () => {
    const result = await run("ls /tmp");
    assert.ok(typeof result === "string");
  });

  it("still rejects /etc even with multiple allowed dirs", async () => {
    await assert.rejects(
      () => run("ls /etc"),
      /resolves outside the allowed directories/,
    );
  });

  it("allows project files alongside tmp access", async () => {
    const result = await run("cat package.json");
    assert.ok(result.includes("name"));
  });
});

describe("bash tool with command protection", async () => {
  const tool = await createBashTool({
    workspace: { primaryDir: baseDir, allowedDirs: [baseDir, "/tmp"] },
  });

  async function run(command: string) {
    return tool.execute(
      { command, cwd: baseDir, timeout: 1000 },
      { toolCallId: "t1", messages: [] },
    );
  }

  it("blocks destructive git commands", async () => {
    await assert.rejects(() => run("git reset --hard"), /BLOCKED/);
  });

  it("blocks rm -rf outside temp directories", async () => {
    // Path validation catches this first with a different error
    await assert.rejects(
      () => run("rm -rf /home/test"),
      /resolves outside the allowed directories/,
    );
  });

  it("allows rm -rf /tmp/*", async () => {
    // Should not throw BLOCKED error (may fail for other reasons like permissions)
    try {
      await run("rm -rf /tmp/acai-test-nonexistent-*");
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : String(error);
      assert.ok(
        !errorMessage.includes("BLOCKED"),
        "rm -rf /tmp/* should be allowed",
      );
    }
  });

  it("allows safe git commands", async () => {
    const result = await run("git status");
    assert.ok(result.length >= 0);
  });
});

describe("bash tool output truncation", async () => {
  const tool = await createBashTool({
    workspace: { primaryDir: baseDir, allowedDirs: [baseDir, "/tmp"] },
  });

  async function run(command: string, timeout = 10000) {
    return tool.execute(
      { command, cwd: baseDir, timeout },
      { toolCallId: "t1", messages: [] },
    );
  }

  it("truncates large output", async () => {
    // Generate output larger than 50KB (the MAX_OUTPUT_SIZE is 51,200 bytes)
    // Each line is 10 bytes, so 10000 lines = 100,000 bytes
    const result = await run("yes 'test line' | head -n 10000");
    assert.ok(result.includes("[OUTPUT TRUNCATED"));
    assert.ok(result.length <= 55 * 1024); // Should be close to 50KB + truncation message
  });

  it("does not truncate small output", async () => {
    const result = await run("echo 'small output'");
    assert.ok(!result.includes("[OUTPUT TRUNCATED"));
    assert.ok(result.includes("small output"));
  });

  it("includes truncated output when command fails", async () => {
    // Generate a large error output
    const result = await run(
      "node -e \"for(let i=0; i<10000; i++) console.error('error line ' + i); process.exit(1);\"",
    );
    assert.ok(result.includes("[OUTPUT TRUNCATED"));
  });

  it("preserves beginning of truncated output", async () => {
    const result = await run("yes 'test line' | head -n 10000");
    assert.ok(result.startsWith("test line"));
  });

  it("includes original size in truncation message", async () => {
    const result = await run("yes 'test line' | head -n 10000");
    assert.ok(/\d[\d,]* characters total/.test(result));
  });
});
