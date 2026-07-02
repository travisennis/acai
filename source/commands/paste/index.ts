import Clipboard from "@crosscopy/clipboard";
import style from "../../terminal/style.ts";
import type { Container, Editor, TUI } from "../../tui/index.ts";
import { Text } from "../../tui/index.ts";
import { formatBlock } from "../../utils/formatting.ts";
import { logger } from "../../utils/logger.ts";
import type { CommandOptions, ReplCommand } from "../types.ts";
import {
  detectImageFormatFromBase64,
  extractBase64Content,
  extractMimeTypeFromDataUrl,
  isValidBase64,
} from "./utils.ts";

function resolveMimeType(base64DataUrl: string): string {
  try {
    const dataUrlMimeType = extractMimeTypeFromDataUrl(base64DataUrl);
    const base64Content = extractBase64Content(base64DataUrl);
    const detectedFormat = detectImageFormatFromBase64(base64Content);

    if (detectedFormat === "unknown") {
      logger.warn(
        `Could not detect image format, using data URL MIME type: ${dataUrlMimeType}`,
      );
      return dataUrlMimeType;
    }

    if (dataUrlMimeType !== detectedFormat) {
      logger.warn(
        `Clipboard library reported ${dataUrlMimeType} but actual image format is ${detectedFormat}. Using detected format.`,
      );
    }
    return detectedFormat;
  } catch (error) {
    logger.warn(`Failed to extract MIME type from clipboard image: ${error}`);
    return "image/png";
  }
}

function buildImageDataUrl(
  base64DataUrl: string,
  mimeType: string,
): { dataUrl: string; valid: boolean } {
  if (base64DataUrl.startsWith(`data:${mimeType};base64,`)) {
    return { dataUrl: base64DataUrl, valid: true };
  }
  const base64Content = extractBase64Content(base64DataUrl);
  const correctedDataUrl = `data:${mimeType};base64,${base64Content}`;
  return { dataUrl: correctedDataUrl, valid: isValidBase64(correctedDataUrl) };
}

interface PasteContext {
  tui: TUI;
  container: Container;
  editor: Editor;
}

function showMessage(ctx: PasteContext, message: string): void {
  ctx.container.addChild(new Text(message, 1, 0));
  ctx.tui.requestRender();
  ctx.editor.setText("");
}

async function handleImagePaste(
  ctx: PasteContext,
  promptManager: CommandOptions["promptManager"],
): Promise<"continue"> {
  const base64DataUrl = await Clipboard.getImageBase64();

  if (!isValidBase64(base64DataUrl)) {
    showMessage(
      ctx,
      style.red(
        "Invalid base64 data in clipboard. The image data may be corrupted.",
      ),
    );
    return "continue";
  }

  const mimeType = resolveMimeType(base64DataUrl);
  const { dataUrl, valid } = buildImageDataUrl(base64DataUrl, mimeType);

  if (!valid) {
    showMessage(
      ctx,
      style.red(
        "Failed to correct base64 data format. The image data may be corrupted.",
      ),
    );
    return "continue";
  }

  promptManager.addContext({
    type: "file",
    data: dataUrl,
    mediaType: mimeType,
  });

  showMessage(
    ctx,
    style.green("Image from clipboard will be added to your next prompt."),
  );
  return "continue";
}

export const pasteCommand = ({
  modelManager,
  promptManager,
  tokenCounter,
}: CommandOptions): ReplCommand => {
  return {
    command: "/paste",
    description:
      "Pastes image or text content from the clipboard into the next prompt.",

    getSubCommands: () => Promise.resolve([]),
    async handle(
      _args: string[],
      {
        tui,
        container,
        editor,
      }: { tui: TUI; container: Container; editor: Editor },
    ): Promise<"continue" | "use"> {
      const ctx = { tui, container, editor };
      try {
        if (Clipboard.hasImage()) {
          return await handleImagePaste(ctx, promptManager);
        }

        const clipboardContent = await Clipboard.getText();
        if (!clipboardContent || clipboardContent.trim() === "") {
          container.addChild(
            new Text(style.yellow("Clipboard is empty."), 0, 1),
          );
          tui.requestRender();
          editor.setText("");
          return "continue";
        }

        const content = formatBlock(
          clipboardContent,
          "clipboard",
          modelManager.getModelMetadata("repl").promptFormat,
        );

        promptManager.addContext(content);

        const tokenCount = tokenCounter.count(content);

        showMessage(
          ctx,
          style.green(
            `Clipboard content will be added to your next prompt. (${tokenCount} tokens)`,
          ),
        );
        return "continue";
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        showMessage(
          ctx,
          style.red(`Error processing clipboard content: ${message}`),
        );
        logger.error(error, "Paste command error:");
        return "continue";
      }
    },
  };
};
