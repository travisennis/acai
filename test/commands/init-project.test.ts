import { strict as assert } from "node:assert";
import { rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { describe, it } from "node:test";
import {
  ensureConfigFile,
  ensureProjectDirectory,
  isDevelopmentDirectory,
} from "../../source/commands/init-project/utils.ts";

function cleanupTestDir(testDir: string): void {
  try {
    rmSync(testDir, { recursive: true, force: true });
  } catch {
    // Ignore cleanup errors
  }
  const agentsDir = join(dirname(testDir), ".agents");
  try {
    rmSync(agentsDir, { recursive: true, force: true });
  } catch {
    // Ignore cleanup errors
  }
}

describe("init-project/utils.ts", () => {
  describe("isDevelopmentDirectory", () => {
    it("returns true for development directory", () => {
      const result = isDevelopmentDirectory("/Users/travisennis/Projects/acai");
      assert.strictEqual(result, true);
    });

    it("returns false for other directories", () => {
      const result = isDevelopmentDirectory(
        "/Users/travisennis/Projects/my-app",
      );
      assert.strictEqual(result, false);
    });

    it("returns false for empty string", () => {
      const result = isDevelopmentDirectory("");
      assert.strictEqual(result, false);
    });
  });

  describe("ensureProjectDirectory", () => {
    it("creates directory and subdirectories when they do not exist", () => {
      const testDir = "/tmp/acai-test-init-project-1";
      cleanupTestDir(testDir);

      const result = ensureProjectDirectory(testDir);

      assert.ok(result.created.length > 0);
      assert.strictEqual(result.existing.length, 0);
      assert.ok(result.created.includes(".acai/"));
      assert.ok(result.created.includes(".agents/skills/"));

      cleanupTestDir(testDir);
    });

    it("marks existing directories as existing", () => {
      const testDir = "/tmp/acai-test-init-project-2";
      cleanupTestDir(testDir);
      ensureProjectDirectory(testDir);

      const result = ensureProjectDirectory(testDir);

      assert.strictEqual(result.created.length, 0);
      assert.ok(result.existing.length > 0);
      assert.ok(result.existing.includes(".acai/"));
      assert.ok(result.existing.includes(".agents/skills/"));

      cleanupTestDir(testDir);
    });
  });

  describe("ensureConfigFile", () => {
    it("creates config file when it does not exist", () => {
      const testDir = "/tmp/acai-test-init-project-3";
      cleanupTestDir(testDir);
      ensureProjectDirectory(testDir);

      const result = ensureConfigFile(testDir);

      assert.strictEqual(result.created.length, 1);
      assert.strictEqual(result.existing.length, 0);
      assert.ok(result.created.includes(".acai/acai.json"));

      cleanupTestDir(testDir);
    });

    it("marks existing config file as existing", () => {
      const testDir = "/tmp/acai-test-init-project-4";
      cleanupTestDir(testDir);
      ensureProjectDirectory(testDir);
      ensureConfigFile(testDir);

      const result = ensureConfigFile(testDir);

      assert.strictEqual(result.created.length, 0);
      assert.strictEqual(result.existing.length, 1);
      assert.ok(result.existing.includes(".acai/acai.json"));

      cleanupTestDir(testDir);
    });
  });
});
