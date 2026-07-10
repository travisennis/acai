import path from "node:path";
import type { Container, Editor, TUI } from "../../tui/index.ts";
import { Spacer, Text } from "../../tui/index.ts";
import type { CommandOptions, ReplCommand } from "../types.ts";
import {
  ensureConfigFile,
  ensureProjectDirectory,
  isDevelopmentDirectory,
} from "./utils.ts";

export const initProjectCommand = (_options: CommandOptions): ReplCommand => {
  return {
    command: "/init-project",
    description:
      "Initialize a new acai project in the current directory. Creates missing directories and files without overwriting existing ones.",

    getSubCommands: () => Promise.resolve([]),

    async handle(
      _args: string[],
      {
        tui,
        container,
        editor,
      }: { tui: TUI; container: Container; editor: Editor },
    ): Promise<"continue" | "use"> {
      const currentDir = process.cwd();

      if (isDevelopmentDirectory(currentDir)) {
        container.addChild(
          new Text(
            "Cannot initialize project in acai development directory.",
            1,
            0,
          ),
        );
        container.addChild(
          new Text("Run this command in a different project directory.", 2, 0),
        );
        tui.requestRender();
        editor.setText("");
        return "continue";
      }

      const projectDir = path.join(currentDir, ".acai");
      const dirResult = ensureProjectDirectory(projectDir);
      const configResult = ensureConfigFile(projectDir);

      const created = [...dirResult.created, ...configResult.created];
      const existing = [...dirResult.existing, ...configResult.existing];

      container.addChild(new Text("Initializing acai project...", 1, 0));

      if (created.length > 0) {
        container.addChild(new Spacer(1));
        container.addChild(new Text("Created:", 1, 0));
        for (const item of created) {
          container.addChild(new Text(`  ${item}`, 2, 0));
        }
      }

      if (existing.length > 0) {
        container.addChild(new Spacer(1));
        container.addChild(new Text("Already existed:", 1, 0));
        for (const item of existing) {
          container.addChild(new Text(`  ${item}`, 2, 0));
        }
      }

      if (created.length === 0 && existing.length > 0) {
        container.addChild(new Spacer(1));
        container.addChild(
          new Text("Project already fully initialized. No changes made.", 1, 0),
        );
      }

      container.addChild(new Spacer(1));
      container.addChild(
        new Text("Project initialized successfully. You can now:", 1, 0),
      );
      container.addChild(
        new Text("  • Configure settings in .acai/acai.json", 2, 0),
      );
      container.addChild(
        new Text("  • Add project skills to .agents/skills/", 2, 0),
      );

      tui.requestRender();
      editor.setText("");
      return "continue";
    },
  };
};
