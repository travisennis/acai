import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { defaultConfig } from "../../config/index.ts";

const DEVELOPMENT_DIRECTORY = "/Users/travisennis/Projects/acai";

interface InitializationResult {
  created: string[];
  existing: string[];
}

export function ensureProjectDirectory(
  projectDir: string,
): InitializationResult {
  const created: string[] = [];
  const existing: string[] = [];

  if (!existsSync(projectDir)) {
    mkdirSync(projectDir, { recursive: true });
    created.push(".acai/");
  } else {
    existing.push(".acai/");
  }

  const agentsSkillsDir = path.join(
    path.dirname(projectDir),
    ".agents",
    "skills",
  );
  if (!existsSync(agentsSkillsDir)) {
    mkdirSync(agentsSkillsDir, { recursive: true });
    created.push(".agents/skills/");
  } else {
    existing.push(".agents/skills/");
  }

  return { created, existing };
}

export function ensureConfigFile(projectDir: string): InitializationResult {
  const created: string[] = [];
  const existing: string[] = [];

  const configPath = path.join(projectDir, "acai.json");
  if (!existsSync(configPath)) {
    writeFileSync(configPath, JSON.stringify(defaultConfig, null, 2), "utf8");
    created.push(".acai/acai.json");
  } else {
    existing.push(".acai/acai.json");
  }

  return { created, existing };
}

export function isDevelopmentDirectory(dir: string): boolean {
  return dir === DEVELOPMENT_DIRECTORY;
}
