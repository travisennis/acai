import EventEmitter from "node:events";
import { devToolsMiddleware } from "@ai-sdk/devtools";
import type { LanguageModelV4 } from "@ai-sdk/provider";
import { type LanguageModelMiddleware, wrapLanguageModel } from "ai";
import {
  auditMessage,
  cacheMiddleware,
  createRateLimitMiddleware,
} from "../middleware/index.ts";
import {
  languageModel,
  type ModelMetadata,
  type ModelName,
  modelRegistry,
} from "./providers.ts";

function getLanguageModel({
  model,
  app,
  stateDir,
  devtoolsEnabled = false,
}: {
  model: ModelName;
  app: string;
  stateDir: string;
  devtoolsEnabled?: boolean;
}) {
  const middleware: LanguageModelMiddleware[] = [
    cacheMiddleware,
    createRateLimitMiddleware({ requestsPerMinute: 30 }),
    auditMessage({ filePath: stateDir, app }),
  ];

  if (devtoolsEnabled) {
    middleware.push(devToolsMiddleware());
  }

  const langModel = wrapLanguageModel({
    model: languageModel(model),
    middleware,
  });

  return langModel;
}

function getModelMetadata({ model }: { model: ModelName }) {
  const modelMetadata = modelRegistry[model];
  return modelMetadata;
}

type App =
  | "repl"
  | "cli"
  | "title-conversation"
  | "conversation-summarizer"
  | "conversation-analyzer"
  | "tool-repair"
  | "init-project";

interface ModelManagerEvents {
  "set-model": [app: App, model: ModelName];
}

export class ModelManager extends EventEmitter<ModelManagerEvents> {
  private modelMap: Map<App, LanguageModelV4>;
  private modelMetadataMap: Map<App, ModelMetadata>;
  private stateDir: string;
  private devtoolsEnabled: boolean;
  constructor({
    stateDir,
    devtoolsEnabled = false,
  }: { stateDir: string; devtoolsEnabled?: boolean }) {
    super();
    this.modelMap = new Map();
    this.modelMetadataMap = new Map();
    this.stateDir = stateDir;
    this.devtoolsEnabled = devtoolsEnabled;
  }

  setModel(app: App, model: ModelName) {
    this.modelMap.set(
      app,
      getLanguageModel({
        model,
        app,
        stateDir: this.stateDir,
        devtoolsEnabled: this.devtoolsEnabled,
      }),
    );
    const modelMetadata = getModelMetadata({ model });
    if (modelMetadata) {
      this.modelMetadataMap.set(app, modelMetadata);
    }
    this.emit("set-model", app, model);
  }

  getModel(app: App) {
    const model = this.modelMap.get(app);
    if (!model) {
      throw new Error("Model not initialized.");
    }
    return model;
  }

  getModelMetadata(app: App) {
    const metadata = this.modelMetadataMap.get(app);
    if (!metadata) {
      throw new Error("Model not initialized.");
    }
    return metadata;
  }
}
