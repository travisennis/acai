#!/usr/bin/env node

/**
 * Check that file references in project markdown files resolve.
 *
 * Checks two patterns (excluding content inside fenced code blocks):
 * 1. Standard markdown links: [text](path) — resolved relative to the file
 * 2. Backtick-quoted paths — resolved relative to project root
 *
 * Ignores node_modules/, .git/, and dist/.
 * Returns exit code 1 if any broken references are found.
 */

import fs from "node:fs";
import path from "node:path";

const ROOT = process.cwd();

// Only check backtick paths starting with these prefixes (project roots)
const PROJECT_DIR_PREFIXES = [
  "docs/",
  "source/",
  "scripts/",
  ".agents/",
  ".acai/",
  ".github/",
  "test/",
  "bin/",
  ".husky/",
  "AGENTS.md",
  "ARCHITECTURE.md",
  "CONTRIBUTING.md",
  "README.md",
  "package.json",
  "tsconfig.json",
  "tsconfig.build.json",
  "biome.json",
  ".env.example",
  ".env.template",
];

// Skip agent working records, issue templates, scratch files
function isInScope(filePath) {
  const rel = path.relative(ROOT, filePath);
  // Skip agent artifacts
  if (rel.startsWith(".agents")) return false;
  if (rel.startsWith(".acai")) return false;
  // Skip GitHub templates
  if (rel.startsWith(".github")) return false;
  // Skip scratch/planning files
  const base = path.basename(filePath);
  const skipFiles = new Set([
    "acai-docs-audit-report.md",
    "docs-update.md",
    "improvements.md",
    "plan.md",
    "prompt.md",
    "research.md",
    "review.md",
    "TODO.md",
  ]);
  if (skipFiles.has(base)) return false;
  return true;
}

function shouldCheckBacktickPath(ref) {
  for (const prefix of PROJECT_DIR_PREFIXES) {
    if (ref.startsWith(prefix)) return true;
  }
  return false;
}

/**
 * Strip fenced code blocks from content, returning only the non-code spans.
 */
function extractNonCodeSpans(content) {
  const spans = [];
  let i = 0;
  let inFenced = false;

  while (i < content.length) {
    if (!inFenced) {
      const fenceStart = content.indexOf("```", i);
      if (fenceStart === -1) {
        spans.push({ start: i, end: content.length });
        break;
      }
      spans.push({ start: i, end: fenceStart });
      const lineEnd = content.indexOf("\n", fenceStart);
      i = lineEnd === -1 ? content.length : lineEnd + 1;
      inFenced = true;
    } else {
      const fenceEnd = content.indexOf("```", i);
      if (fenceEnd === -1) break;
      const lineEnd = content.indexOf("\n", fenceEnd);
      i = lineEnd === -1 ? content.length : lineEnd + 1;
      inFenced = false;
    }
  }

  return spans.map((s) => content.slice(s.start, s.end));
}

function collectMdFiles(dir, results = []) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (entry.name === "node_modules" || entry.name === "dist" || entry.name === ".git") continue;
    if (entry.isDirectory()) {
      collectMdFiles(fullPath, results);
    } else if (entry.name.endsWith(".md")) {
      results.push(fullPath);
    }
  }
  return results;
}

async function main() {
  let exitCode = 0;
  const mdFiles = collectMdFiles(ROOT);

  for (const file of mdFiles) {
    const content = fs.readFileSync(file, "utf8");
    const dir = path.dirname(file);
    const relPath = path.relative(ROOT, file);

    const nonCodeSpans = extractNonCodeSpans(content);
    const nonCodeContent = nonCodeSpans.join("\n");

    // Skip non-project files
    if (!isInScope(file)) continue;

    const brokenMdLinks = [];
    const brokenBacktickPaths = new Set();

    // 1. Check markdown links: [text](path) — resolved relative to file
    const mdLinkRe = /\[([^\]]*)\]\(([^)]+)\)/g;
    let m;
    while ((m = mdLinkRe.exec(nonCodeContent)) !== null) {
      const ref = m[2].split("#")[0].trim();
      if (ref.startsWith("http") || ref.startsWith("#") || ref.startsWith("/") || ref === "") continue;
      const resolved = path.resolve(dir, ref);
      if (!fs.existsSync(resolved)) {
        brokenMdLinks.push(ref);
      }
    }

    // 2. Check backtick paths — resolved relative to project root
    const backtickRe = /`([^`]+)`/g;
    while ((m = backtickRe.exec(nonCodeContent)) !== null) {
      const ref = m[1].trim();
      const anchorIdx = ref.indexOf("#");
      const pathPart = anchorIdx >= 0 ? ref.slice(0, anchorIdx) : ref;
      if (!shouldCheckBacktickPath(pathPart)) continue;
      // Skip paths with line numbers, wildcards, or template placeholders
      if (/:\d/.test(pathPart)) continue;
      if (/[*?]/.test(pathPart)) continue;
      if (/<[^>]+>/.test(pathPart)) continue;
      // Skip user-created runtime subdirectories (won't exist at checkout)
      if (/^\.acai\/(tools|rules|selections)\b/.test(pathPart)) continue;
      // Skip paths in ADRs referencing files that existed when the ADR was written
      if (relPath.startsWith("docs/adr/") && pathPart.startsWith("source/")) continue;
      // Resolve relative to project root (backtick paths are root-relative)
      const resolved = path.resolve(ROOT, pathPart);
      if (!fs.existsSync(resolved)) {
        brokenBacktickPaths.add(pathPart);
      }
    }

    if (brokenMdLinks.length > 0 || brokenBacktickPaths.size > 0) {
      console.log(`\n\x1b[33m${relPath}\x1b[0m`);
      if (brokenMdLinks.length > 0) {
        console.log("  Broken markdown links:");
        brokenMdLinks.forEach((l) => console.log(`    \x1b[31m✗\x1b[0m ${l}`));
      }
      if (brokenBacktickPaths.size > 0) {
        console.log("  Broken backtick path references:");
        brokenBacktickPaths.forEach((l) =>
          console.log(`    \x1b[31m✗\x1b[0m ${l}`)
        );
      }
      exitCode = 1;
    }
  }

  if (exitCode === 0) {
    console.log("\x1b[32m✓ All file references resolve.\x1b[0m");
  }
  process.exit(exitCode);
}

main().catch((err) => {
  console.error("Link check failed:", err);
  process.exit(1);
});
