import { platform } from "node:os";
import { isStepCount, streamText } from "ai";
import style from "../../terminal/style.ts";
import { initTools } from "../../tools/index.ts";
import { toAiSdkTools } from "../../tools/utils.ts";
import type { Container, Editor, TUI } from "../../tui/index.ts";
import { Markdown, Spacer, Text } from "../../tui/index.ts";
import { inGitDirectory } from "../../utils/git.ts";
import type { CommandOptions, ReplCommand } from "../types.ts";

const initPrompt = `Please analyze this codebase and create a AGENTS.md file containing:
1. An overview of the project including how the project is structured and the tech stack used.
2. Build/lint/test commands - especially for running a single test
3. Directions on how to run the app
4. Custom tools defined in ./.acai/tools
3. Code style guidelines including imports, formatting, types, naming conventions, error handling, etc.
4. Commit format (does the repository use Conventional Commits?)
5. Branch strategy
6. PR requirements

The file you create will be given to agentic coding agents (such as yourself) that operate in this repository. Make it about 50 lines long.

If there's already a AGENTS.md, improve it.
If there are Cursor rules (in .cursor/rules/ or .cursorrules), Copilot rules (in .github/copilot-instructions.md or .github/instructions/), or Windsurf rules (in .windsurf/rules), make sure to include them.

Your current working directory is ${process.cwd()}
Is directory a git repo: ${(await inGitDirectory()) ? "Yes" : "No"}
Platform: ${platform()}`;

export const initCommand = ({
  modelManager,
  workspace,
}: CommandOptions): ReplCommand => {
  return {
    command: "/init",
    description: "Creates the AGENTS.md file.",

    getSubCommands: () => Promise.resolve([]),

    async handle(
      _args: string[],
      {
        tui,
        container,
        editor,
      }: { tui: TUI; container: Container; editor: Editor },
    ): Promise<"continue" | "use"> {
      container.addChild(
        new Text("Initializing project and creating AGENTS.md...", 1, 1),
      );
      tui.requestRender();

      const tools = await initTools({
        workspace,
      });

      const result = streamText({
        model: modelManager.getModel("init-project"),
        temperature: 0.5,
        prompt: initPrompt,
        stopWhen: isStepCount(40),
        tools: toAiSdkTools(tools),
      });

      container.addChild(new Spacer(1));

      let output = "";
      const t = new Markdown(output, { paddingX: 1, paddingY: 0 });
      container.addChild(t);
      for await (const text of result.textStream) {
        output += text;
        // Update the display with the latest output
        t.setText(output);
        tui.requestRender();
      }

      container.addChild(new Spacer(1));

      container.addChild(
        new Text(style.green("AGENTS.md file created successfully"), 1, 0),
      );
      tui.requestRender();
      editor.setText("");
      return "continue";
    },
  };
};
