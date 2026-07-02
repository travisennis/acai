/* biome-ignore-all lint/suspicious/noExplicitAny: internal function uses simplified types */
import { generateText, type TextPart } from "ai";
import { SessionManager } from "../../sessions/manager.ts";
import {
  getTerminalSize,
  isArrowDown,
  isArrowUp,
  isEnter,
  isEscape,
  isShiftTab,
  isTab,
  setTerminalTitle,
} from "../../terminal/control.ts";
import style from "../../terminal/style.ts";
import type { Editor, TUI } from "../../tui/index.ts";
import { Container, Input, Markdown, Spacer, Text } from "../../tui/index.ts";
import type { CommandOptions, ReplCommand } from "../types.ts";
import type { ConversationHistory } from "./types.ts";
import { exportConversation } from "./utils.ts";

async function summarizeConversation(
  history: { title: string; messages: any[] },
  modelManager: CommandOptions["modelManager"],
  tokenTracker: CommandOptions["tokenTracker"],
): Promise<string> {
  const systemPrompt = `You are an expert at summarizing conversations between a coding assistant and a user. Your task is to provide a clear, concise summary of the conversation that captures:

1. The main topic and objectives
2. Key decisions and solutions discussed
3. Tools and techniques used
4. Overall outcome or current status

Keep the summary focused and informative, around 3-5 paragraphs. Use plain text without markdown formatting.`;

  const conversationText = history.messages
    .map((message: any) => {
      const role = message.role.toUpperCase();
      let content = "";

      if (Array.isArray(message.content)) {
        content = message.content
          .filter(
            (part: TextPart | { type: string; text?: string }) =>
              part.type === "text" && part.text?.trim(),
          )
          .map((part: TextPart | { type: string; text?: string }) => part.text)
          .join("\n");
      } else if (typeof message.content === "string") {
        content = message.content;
      }

      return `${role}: ${content}`;
    })
    .filter((text: string | undefined) => text?.trim())
    .join("\n\n");

  const { text, usage } = await generateText({
    model: modelManager.getModel("conversation-summarizer"),
    instructions: systemPrompt,
    prompt: `Please summarize this conversation:\n\n${conversationText}`,
  });

  tokenTracker.trackUsage("conversation-summarizer", usage);

  const results: string[] = [];
  results.push(`# Summary of "${history.title}":`);
  results.push("");
  results.push(text);
  results.push("");
  return results.join("\n");
}

export const historyCommand = ({
  sessionManager,
  config,
  modelManager,
  tokenTracker,
}: CommandOptions): ReplCommand => {
  return {
    command: "/history",
    description: "Browse and manage previous conversations.",
    getSubCommands: () => Promise.resolve([]),

    async handle(
      _args: string[],
      {
        tui,
        container,
        inputContainer,
        editor,
      }: {
        tui: TUI;
        container: Container;
        inputContainer: Container;
        editor: Editor;
      },
    ): Promise<"continue" | "use"> {
      const appDir = config.app;
      const sessionsDir = await appDir.ensurePath("sessions");

      // Load all histories (use a large number to get all)
      const histories = await SessionManager.load(sessionsDir, 1000);

      if (histories.length === 0) {
        container.addChild(
          new Text(style.yellow("No previous conversations found."), 0, 1),
        );
        tui.requestRender();
        editor.setText("");
        return "continue";
      }

      // Create conversation selector
      const conversationSelector = new ConversationSelectorComponent(
        histories,
        async (conversation) => {
          // Handle conversation selection
          const actionSelector = new ActionSelectorComponent(
            conversation,
            async (action) => {
              // Handle action selection
              switch (action) {
                case "resume":
                  sessionManager.restore(conversation);
                  container.addChild(
                    new Text(
                      style.green(
                        `Resuming conversation: ${conversation.title}`,
                      ),
                      0,
                      1,
                    ),
                  );
                  setTerminalTitle(
                    conversation.title || `acai: ${process.cwd()}`,
                  );
                  // Trigger TUI reconstruction to show restored conversation
                  tui.onReconstructSession?.();
                  break;

                case "export":
                  try {
                    const destFile = await exportConversation(conversation);
                    container.addChild(
                      new Text(
                        style.green(
                          `Conversation exported successfully: ${destFile}`,
                        ),
                        1,
                        0,
                      ),
                    );
                  } catch (error) {
                    container.addChild(
                      new Text(
                        style.red(`Failed to export conversation: ${error}`),
                        1,
                        0,
                      ),
                    );
                  }
                  break;

                case "summarize":
                  try {
                    const result = await summarizeConversation(
                      conversation,
                      modelManager,
                      tokenTracker,
                    );
                    container.addChild(
                      new Markdown(result, {
                        customBgRgb: {
                          r: 52,
                          g: 53,
                          b: 65,
                        },
                      }),
                    );
                  } catch (error) {
                    container.addChild(
                      new Text(
                        style.red(`Failed to summarize conversation: ${error}`),
                        1,
                        0,
                      ),
                    );
                  }
                  break;
              }

              // Hide selectors and show editor again
              hideHistorySelectors(inputContainer, editor, tui);
              tui.requestRender();
            },
            () => {
              // Cancel action selection - go back to conversation selection
              hideHistorySelectors(inputContainer, editor, tui);
              inputContainer.addChild(conversationSelector);
              tui.setFocus(conversationSelector);
              tui.requestRender();
            },
          );

          // Replace conversation selector with action selector
          inputContainer.clear();
          inputContainer.addChild(actionSelector);
          tui.setFocus(actionSelector);
          tui.requestRender();
        },
        () => {
          // Cancel conversation selection - just hide selector
          hideHistorySelectors(inputContainer, editor, tui);
          tui.requestRender();
        },
      );

      // Replace editor with conversation selector
      inputContainer.clear();
      inputContainer.addChild(conversationSelector);
      tui.setFocus(conversationSelector);
      tui.requestRender();
      return "continue";
    },
  };
};

function hideHistorySelectors(
  editorContainer: Container,
  editor: Editor,
  tui: TUI,
): void {
  // Replace selector with editor in the container
  editorContainer.clear();
  editorContainer.addChild(editor);
  tui.setFocus(editor);
}

/**
 * Component that renders a conversation selector with search
 */
class ConversationSelectorComponent extends Container {
  private searchInput: Input;
  private listContainer: Container;
  private allConversations: ConversationHistory[] = [];
  private filteredConversations: ConversationHistory[] = [];
  private selectedIndex = 0;
  private onSelectCallback: (conversation: ConversationHistory) => void;
  private onCancelCallback: () => void;

  constructor(
    conversations: ConversationHistory[],
    onSelect: (conversation: ConversationHistory) => void,
    onCancel: () => void,
  ) {
    super();

    this.onSelectCallback = onSelect;
    this.onCancelCallback = onCancel;

    // Load all conversations
    this.allConversations = conversations;
    this.filteredConversations = conversations;

    const { columns } = getTerminalSize();

    // Add top border
    this.addChild(new Text(style.blue("─".repeat(columns)), 0, 0));
    this.addChild(new Spacer(1));

    // Create search input
    this.searchInput = new Input();
    // Note: setPlaceholder not available on Input component
    this.searchInput.onSubmit = () => {
      // Enter on search input selects the first filtered item
      if (this.filteredConversations[this.selectedIndex]) {
        this.handleSelect(this.filteredConversations[this.selectedIndex]);
      }
    };
    this.addChild(this.searchInput);

    this.addChild(new Spacer(1));

    // Create list container
    this.listContainer = new Container();
    this.addChild(this.listContainer);

    this.addChild(new Spacer(1));

    // Add bottom border
    this.addChild(new Text(style.blue("─".repeat(columns)), 0, 0));

    // Initial render
    this.updateList();
  }

  private filterConversations(query: string): void {
    if (!query.trim()) {
      this.filteredConversations = this.allConversations;
    } else {
      const searchTokens = query
        .toLowerCase()
        .split(/\s+/)
        .filter((t) => t);
      this.filteredConversations = this.allConversations.filter(
        (conversation) => {
          const searchText =
            `${conversation.title} ${conversation.sessionId}`.toLowerCase();
          return searchTokens.every((token) => searchText.includes(token));
        },
      );
    }

    this.selectedIndex = Math.min(
      this.selectedIndex,
      Math.max(0, this.filteredConversations.length - 1),
    );
    this.updateList();
  }

  private updateList(): void {
    this.listContainer.clear();

    const maxVisible = 10;
    const startIndex = Math.max(
      0,
      Math.min(
        this.selectedIndex - Math.floor(maxVisible / 2),
        this.filteredConversations.length - maxVisible,
      ),
    );
    const endIndex = Math.min(
      startIndex + maxVisible,
      this.filteredConversations.length,
    );

    // Show visible slice of filtered conversations
    for (let i = startIndex; i < endIndex; i++) {
      const conversation = this.filteredConversations[i];
      if (!conversation) continue;

      const isSelected = i === this.selectedIndex;

      let line = "";
      if (isSelected) {
        const prefix = style.blue("→ ");
        const title = conversation.title;
        const date = conversation.updatedAt.toLocaleString();
        const messages = conversation.messages.length;
        line = `${prefix + style.blue(title)} ${style.gray(`(${date}) - ${messages} messages`)}`;
      } else {
        const title = `  ${conversation.title}`;
        const date = conversation.updatedAt.toLocaleString();
        const messages = conversation.messages.length;
        line = `${title} ${style.gray(`(${date}) - ${messages} messages`)}`;
      }

      this.listContainer.addChild(new Text(line, 0, 0));
    }

    // Add scroll indicator if needed
    if (startIndex > 0 || endIndex < this.filteredConversations.length) {
      const scrollInfo = style.gray(
        `  (${this.selectedIndex + 1}/${this.filteredConversations.length})`,
      );
      this.listContainer.addChild(new Text(scrollInfo, 0, 0));
    }

    // Show "no results" if empty
    if (this.filteredConversations.length === 0) {
      this.listContainer.addChild(
        new Text(style.gray("  No matching conversations"), 0, 0),
      );
    }
  }

  wantsNavigationKeys(): boolean {
    return true;
  }

  handleInput(keyData: string): void {
    if (isArrowUp(keyData) || isShiftTab(keyData)) {
      this.selectedIndex = Math.max(0, this.selectedIndex - 1);
      this.updateList();
    } else if (isArrowDown(keyData) || isTab(keyData)) {
      this.selectedIndex = Math.min(
        this.filteredConversations.length - 1,
        this.selectedIndex + 1,
      );
      this.updateList();
    } else if (isEnter(keyData)) {
      const selectedConversation =
        this.filteredConversations[this.selectedIndex];
      if (selectedConversation) {
        this.handleSelect(selectedConversation);
      }
    } else if (isEscape(keyData)) {
      this.onCancelCallback();
    } else {
      this.searchInput.handleInput(keyData);
      this.filterConversations(this.searchInput.getValue());
    }
  }

  private handleSelect(conversation: ConversationHistory): void {
    this.onSelectCallback(conversation);
  }

  getSearchInput(): Input {
    return this.searchInput;
  }
}

/**
 * Component that renders an action selector for a conversation
 */
class ActionSelectorComponent extends Container {
  private selectedIndex = 0;
  private conversationTitle: string;
  private onSelectCallback: (action: "resume" | "export" | "summarize") => void;
  private onCancelCallback: () => void;

  constructor(
    conversation: ConversationHistory,
    onSelect: (action: "resume" | "export" | "summarize") => void,
    onCancel: () => void,
  ) {
    super();

    this.conversationTitle = conversation.title;
    this.onSelectCallback = onSelect;
    this.onCancelCallback = onCancel;

    const { columns } = getTerminalSize();

    // Add top border
    this.addChild(new Text(style.blue("─".repeat(columns)), 0, 0));
    this.addChild(new Spacer(1));

    // Show conversation title
    this.addChild(new Text(`Selected: ${conversation.title}`, 0, 0));
    this.addChild(new Spacer(1));

    // Create action list
    this.addChild(new Text("Choose an action:", 0, 0));
    this.addChild(new Spacer(1));

    const actions = [
      { name: "Resume - Continue this conversation", value: "resume" },
      { name: "Export - Save as markdown file", value: "export" },
      {
        name: "Summarize - Generate AI summary of conversation",
        value: "summarize",
      },
    ];

    actions.forEach((action, index) => {
      const isSelected = index === this.selectedIndex;
      let line = "";
      if (isSelected) {
        line = `${style.blue("→ ") + style.blue(action.name)}`;
      } else {
        line = `  ${action.name}`;
      }
      this.addChild(new Text(line, 0, 0));
    });

    this.addChild(new Spacer(1));

    // Add bottom border
    this.addChild(new Text(style.blue("─".repeat(columns)), 0, 0));
  }

  wantsNavigationKeys(): boolean {
    return true;
  }

  handleInput(keyData: string): void {
    if (isArrowUp(keyData) || isShiftTab(keyData)) {
      this.selectedIndex = Math.max(0, this.selectedIndex - 1);
      this.updateList();
    } else if (isArrowDown(keyData) || isTab(keyData)) {
      this.selectedIndex = Math.min(2, this.selectedIndex + 1);
      this.updateList();
    } else if (isEnter(keyData)) {
      const actions = ["resume", "export", "summarize"] as const;
      const selectedAction = actions[this.selectedIndex];
      if (selectedAction) {
        this.handleSelect(selectedAction);
      }
    } else if (isEscape(keyData)) {
      this.onCancelCallback();
    }
  }

  private updateList(): void {
    // Clear and rebuild the action list
    this.clear();

    const { columns } = getTerminalSize();

    // Add top border
    this.addChild(new Text(style.blue("─".repeat(columns)), 0, 0));
    this.addChild(new Spacer(1));

    // Show conversation title
    this.addChild(new Text(`Selected: ${this.conversationTitle}`, 0, 0));
    this.addChild(new Spacer(1));

    // Create action list
    this.addChild(new Text("Choose an action:", 0, 0));
    this.addChild(new Spacer(1));

    const actions = [
      { name: "Resume - Continue this conversation", value: "resume" },
      { name: "Export - Save as markdown file", value: "export" },
      {
        name: "Summarize - Generate AI summary of conversation",
        value: "summarize",
      },
    ];

    actions.forEach((action, index) => {
      const isSelected = index === this.selectedIndex;
      let line = "";
      if (isSelected) {
        line = `${style.blue("→ ") + style.blue(action.name)}`;
      } else {
        line = `  ${action.name}`;
      }
      this.addChild(new Text(line, 0, 0));
    });

    this.addChild(new Spacer(1));

    // Add bottom border
    this.addChild(new Text(style.blue("─".repeat(columns)), 0, 0));
  }

  private handleSelect(action: "resume" | "export" | "summarize"): void {
    this.onSelectCallback(action);
  }
}
