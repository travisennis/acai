import { existsSync, mkdirSync } from "node:fs";
import fs from "node:fs/promises";
import { homedir } from "node:os";
import path from "node:path";
import { z } from "zod";
import { jsonParser } from "../utils/parsing.ts";

const PATH_SEPARATOR = process.platform === "win32" ? ";" : ":";

export function parseSkillsPath(skillsPath: string): string[] {
  if (!skillsPath) return [];
  const paths = skillsPath
    .split(PATH_SEPARATOR)
    .map((p) => p.trim())
    .filter((p) => p.length > 0)
    .map((p) => {
      if (p.startsWith("~")) {
        return path.join(homedir(), p.slice(1));
      }
      return path.resolve(p);
    });
  return [...new Set(paths)];
}

export const defaultConfig = {
  loop: {
    maxIterations: 200,
  },
  tools: {
    activeTools: undefined as string[] | undefined,
    maxTokens: 30000,
    maxResults: 30,
    dynamicTools: {
      enabled: true,
      maxTools: 10,
    },
  },
  notify: true,
  readOnlyFiles: [] as string[],
  skills: {
    enabled: true,
    path: "",
  },
  devtools: {
    enabled: false,
  },
  allowedDirs: [] as string[],
  env: {} as Record<string, string>,
} as const;

// Type definitions
const ConfigSchema = z.object({
  logs: z
    .object({
      path: z.string(),
    })
    .optional(),
  loop: z
    .object({
      maxIterations: z.number().default(defaultConfig.loop.maxIterations),
    })
    .optional()
    .default(defaultConfig.loop),
  tools: z
    .object({
      activeTools: z.array(z.string()).optional(),
      maxTokens: z.number().default(defaultConfig.tools.maxTokens),
      maxResults: z.number().default(defaultConfig.tools.maxResults),
      dynamicTools: z
        .object({
          enabled: z
            .boolean()
            .default(defaultConfig.tools.dynamicTools.enabled),
          maxTools: z
            .number()
            .default(defaultConfig.tools.dynamicTools.maxTools),
        })
        .optional()
        .default(defaultConfig.tools.dynamicTools),
    })
    .optional()
    .default(defaultConfig.tools),
  notify: z.boolean().optional().default(defaultConfig.notify),
  readOnlyFiles: z
    .array(z.string())
    .optional()
    .default(defaultConfig.readOnlyFiles),
  skills: z
    .object({
      enabled: z.boolean().optional().default(defaultConfig.skills.enabled),
      path: z.string().optional().default(defaultConfig.skills.path),
    })
    .optional()
    .default(defaultConfig.skills),
  allowedDirs: z
    .array(z.string())
    .optional()
    .default(defaultConfig.allowedDirs),
  devtools: z
    .object({
      enabled: z.boolean().optional().default(defaultConfig.devtools.enabled),
    })
    .optional()
    .default(defaultConfig.devtools),
  env: z.record(z.string(), z.string()).optional().default(defaultConfig.env),
});

export type Config = z.infer<typeof ConfigSchema>;

export class DirectoryProvider {
  private baseDir: string;

  constructor(baseDir: string) {
    this.baseDir = baseDir;
  }

  getPath(subdir?: string): string {
    return subdir ? path.join(this.baseDir, subdir) : this.baseDir;
  }

  // Async-by-default: prefer non-blocking filesystem operations.
  async ensurePath(subdir?: string): Promise<string> {
    const dirPath = this.getPath(subdir);
    await fs.mkdir(dirPath, { recursive: true });
    return dirPath;
  }

  // Synchronous helper for call-sites that require immediate directory availability.
  ensurePathSync(subdir?: string): string {
    const dirPath = this.getPath(subdir);
    mkdirSync(dirPath, { recursive: true });
    return dirPath;
  }

  // Check if directory exists without creating it
  async exists(subdir?: string): Promise<boolean> {
    const dirPath = this.getPath(subdir);
    try {
      await fs.access(dirPath);
      return true;
    } catch {
      return false;
    }
  }

  // Synchronous version of exists check
  existsSync(subdir?: string): boolean {
    const dirPath = this.getPath(subdir);
    return existsSync(dirPath);
  }
}

export class ConfigManager {
  readonly project: DirectoryProvider;
  readonly app: DirectoryProvider;
  private cachedConfig: Config | null = null;
  private configCacheTime = 0;
  private configCacheTtl = 60000; // 60 seconds

  constructor() {
    this.project = new DirectoryProvider(path.join(process.cwd(), ".acai"));
    this.app = new DirectoryProvider(path.join(homedir(), ".acai"));
  }

  private async _readConfig(configPath: string): Promise<Partial<Config>> {
    try {
      const data = await fs.readFile(configPath, "utf8");
      return JSON.parse(data) as Partial<Config>;
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code === "ENOENT" || code === "EPERM" || code === "EACCES") {
        return {};
      }
      throw error;
    }
  }

  // Get merged configuration (project overrides app), cached with TTL
  async getConfig(): Promise<Config> {
    const now = Date.now();
    if (this.cachedConfig && now - this.configCacheTime < this.configCacheTtl) {
      return this.cachedConfig;
    }

    const appConfigPath = path.join(this.app.getPath(), "acai.json");
    const projectConfigPath = path.join(this.project.getPath(), "acai.json");

    const appConfig = await this._readConfig(appConfigPath);
    const projectConfig = await this._readConfig(projectConfigPath);

    const mergedEnv = {
      ...appConfig.env,
      ...projectConfig.env,
    };

    const mergedAllowedDirs = [
      ...(appConfig.allowedDirs ?? []),
      ...(projectConfig.allowedDirs ?? []),
    ];

    const mergedSkills = {
      ...appConfig.skills,
      ...projectConfig.skills,
      path: [appConfig.skills?.path, projectConfig.skills?.path]
        .filter((p): p is string => !!p)
        .join(PATH_SEPARATOR),
    };

    const mergedConfig = {
      ...appConfig,
      ...projectConfig,
      env: mergedEnv,
      allowedDirs: mergedAllowedDirs,
      skills: mergedSkills,
    };

    const result = ConfigSchema.parse(mergedConfig);
    this.cachedConfig = result;
    this.configCacheTime = now;
    return result;
  }

  // Skills settings helpers
  async getSkillsEnabled(): Promise<boolean> {
    const projectConfig = await this.getConfig();
    return projectConfig.skills?.enabled ?? true;
  }

  async setSkillsEnabled(enabled: boolean): Promise<void> {
    const configPath = path.join(this.app.getPath(), "acai.json");
    let configData: Partial<Config> = {};

    try {
      const data = await fs.readFile(configPath, "utf8");
      configData = jsonParser(ConfigSchema.partial()).parse(data);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
        throw error;
      }
    }

    if (!configData.skills) {
      configData.skills = { enabled: true, path: "" };
    }
    configData.skills.enabled = enabled;

    await this.app.ensurePath();
    await fs.writeFile(configPath, JSON.stringify(configData, null, 2));
  }

  async readAgentsFiles(): Promise<{ path: string; content: string }[]> {
    const files: { path: string; content: string }[] = [];

    const agentsPaths = [
      {
        absolute: path.join(this.app.getPath(), "AGENTS.md"),
        relative: "~/.acai/AGENTS.md",
      },
      {
        absolute: path.join(homedir(), ".config", "AGENTS.md"),
        relative: "~/.config/AGENTS.md",
      },
      {
        absolute: path.join(process.cwd(), "AGENTS.md"),
        relative: "./AGENTS.md",
      },
    ];
    for (const { absolute, relative } of agentsPaths) {
      try {
        const content = await fs.readFile(absolute, "utf8");
        files.push({
          path: relative,
          content,
        });
      } catch (_error) {
        // ignore
      }
    }

    return files;
  }

  async writeAgentsFile(rules: string): Promise<void> {
    const agentsPath = path.join(process.cwd(), "AGENTS.md");
    try {
      return await fs.writeFile(agentsPath, rules, "utf8");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") {
        return;
      }
      throw error;
    }
  }
  // Learned rules (global, stored in ~/.acai/rules/learned-rules.md)
  async readLearnedRulesFile(): Promise<string> {
    const rulesPath = path.join(this.app.getPath("rules"), "learned-rules.md");
    try {
      return await fs.readFile(rulesPath, "utf8");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") {
        return "";
      }
      throw error;
    }
  }

  async writeLearnedRulesFile(rules: string): Promise<void> {
    const rulesDir = await this.app.ensurePath("rules");
    const rulesPath = path.join(rulesDir, "learned-rules.md");
    try {
      return await fs.writeFile(rulesPath, rules, "utf8");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") {
        return;
      }
      throw error;
    }
  }

  // Private helper for reading app config
  private async _readAppConfig(configName: string): Promise<Config> {
    const configPath = path.join(this.app.getPath(), `${configName}.json`);
    try {
      const data = await fs.readFile(configPath, "utf8");
      return ConfigSchema.parse(JSON.parse(data));
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") {
        return ConfigSchema.parse(defaultConfig);
      }
      throw error;
    }
  }

  // Ensure default app configuration exists
  async ensureDefaultConfig(configName: string): Promise<Config> {
    const configPath = path.join(this.app.getPath(), `${configName}.json`);

    try {
      await fs.access(configPath);
      return await this._readAppConfig(configName);
    } catch (error) {
      // Only create default config when the file is missing; rethrow on malformed JSON, etc.
      if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
        throw error;
      }
      await this.app.ensurePath();

      await fs.writeFile(configPath, JSON.stringify(defaultConfig, null, 2));
      return ConfigSchema.parse(defaultConfig);
    }
  }

  async writeAppConfig(
    configName: string,
    data: Record<PropertyKey, unknown>,
  ): Promise<void> {
    await this.app.ensurePath();
    const configPath = path.join(this.app.getPath(), `${configName}.json`);
    await fs.writeFile(configPath, JSON.stringify(data, null, 2));
  }
}

// Create a singleton instance
export const config = new ConfigManager();
