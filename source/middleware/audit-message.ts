import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import type {
  LanguageModelV4Prompt,
  LanguageModelV4StreamPart,
  LanguageModelV4TextPart,
} from "@ai-sdk/provider";
import type { LanguageModelMiddleware, LanguageModelUsage } from "ai";

interface AuditRecord {
  model: string;
  app: string;
  messages: LanguageModelV4Prompt;
  usage: LanguageModelUsage;
  timestamp: number;
}

/**
 * Writes an audit record to the specified file, overwriting any existing content.
 * Ensures the directory exists before writing.
 *
 * @param filePath - The path to the file where the audit record will be saved.
 * @param content - The audit record object to write.
 */
const writeAuditRecord = async (
  app: string,
  filePath: string,
  content: AuditRecord,
): Promise<void> => {
  try {
    const now = new Date();
    const path = join(filePath, `${now.toISOString()}-${app}-message.json`);
    // Ensure directory exists
    await mkdir(dirname(path), { recursive: true });
    await writeFile(path, `${JSON.stringify(content, null, 2)}`);
  } catch (error) {
    console.error("Error writing audit file:", error);
    throw error;
  }
};

export const auditMessage = ({
  filePath = "messages",
  app = "default",
}: {
  filePath: string;
  app: string;
}) => {
  const middleware: LanguageModelMiddleware = {
    specificationVersion: "v4",
    wrapGenerate: async ({ doGenerate, params, model }) => {
      const result = await doGenerate();

      const msg: AuditRecord = {
        model: model.modelId,
        app,
        messages: [...params.prompt].concat({
          role: "assistant",
          content: [
            {
              type: "text",
              // biome-ignore lint/suspicious/noExplicitAny: work-around on type issue
              text: (result as any).text,
            } as LanguageModelV4TextPart,
          ],
        }),
        usage: {
          inputTokens: result.usage.inputTokens.total,
          outputTokens: result.usage.outputTokens.total,
          totalTokens:
            (result.usage.inputTokens.total ?? 0) +
            (result.usage.outputTokens.total ?? 0),
          inputTokenDetails: {
            noCacheTokens: result.usage.inputTokens.noCache,
            cacheReadTokens: result.usage.inputTokens.cacheRead,
            cacheWriteTokens: result.usage.inputTokens.cacheWrite,
          },
          outputTokenDetails: {
            textTokens: result.usage.outputTokens.text,
            reasoningTokens: result.usage.outputTokens.reasoning,
          },
        },
        timestamp: Date.now(),
      };

      await writeAuditRecord(app, filePath, msg);

      return result;
    },

    wrapStream: async ({ doStream, params, model }) => {
      const { stream, ...rest } = await doStream();

      let generatedText = "";
      let usage: LanguageModelUsage = {
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        inputTokenDetails: {
          noCacheTokens: 0,
          cacheReadTokens: 0,
          cacheWriteTokens: 0,
        },
        outputTokenDetails: {
          textTokens: 0,
          reasoningTokens: 0,
        },
      };

      const transformStream = new TransformStream<
        LanguageModelV4StreamPart,
        LanguageModelV4StreamPart
      >({
        transform(chunk, controller) {
          if (chunk.type === "text-delta") {
            generatedText += chunk.delta;
          }
          if (chunk.type === "finish") {
            usage = {
              inputTokens: chunk.usage.inputTokens.total,
              outputTokens: chunk.usage.outputTokens.total,
              totalTokens:
                (chunk.usage.inputTokens.total ?? 0) +
                (chunk.usage.outputTokens.total ?? 0),
              inputTokenDetails: {
                noCacheTokens: chunk.usage.inputTokens.noCache,
                cacheReadTokens: chunk.usage.inputTokens.cacheRead,
                cacheWriteTokens: chunk.usage.inputTokens.cacheWrite,
              },
              outputTokenDetails: {
                textTokens: chunk.usage.outputTokens.text,
                reasoningTokens: chunk.usage.outputTokens.reasoning,
              },
            };
          }
          controller.enqueue(chunk);
        },

        async flush() {
          const msg: AuditRecord = {
            model: model.modelId,
            app,
            messages: generatedText
              ? [
                  ...params.prompt,
                  {
                    role: "assistant",
                    content: [
                      {
                        type: "text",
                        text: generatedText,
                      },
                    ],
                  },
                ]
              : [...params.prompt],
            usage,
            timestamp: Date.now(),
          };

          await writeAuditRecord(app, filePath, msg);
        },
      });

      return {
        stream: stream.pipeThrough(transformStream),
        ...rest,
      };
    },
  };
  return middleware;
};
