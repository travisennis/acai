use std::str::FromStr;

use log::error;
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{Diagnostic, Range, Url};

use crate::{
    clients::{
        providers::{ModelConfig, Provider},
        ChatCompletion, CompletionClient,
    },
    config::DataDir,
    models::{Message, Role},
};

use super::embedded_instructions::parse_context;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiCodeAction {
    Instruct,
    Document,
    Fix,
    Optimize,
    Suggest,
    FillInMiddle,
    Test,
}

impl AiCodeAction {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Instruct => "Acai - Instruct",
            Self::Document => "Acai - Document",
            Self::Fix => "Acai - Fix",
            Self::Optimize => "Acai - Optimize",
            Self::Suggest => "Acai - Suggest",
            Self::FillInMiddle => "Acai - Fill in middle",
            Self::Test => "Acai - Test",
        }
    }

    /// Returns the identifier of the command.
    pub const fn identifier(self) -> &'static str {
        match self {
            Self::Instruct => "ai.instruct",
            Self::Document => "ai.document",
            Self::Fix => "ai.fix",
            Self::Optimize => "ai.optimize",
            Self::Suggest => "ai.suggest",
            Self::FillInMiddle => "ai.fillInMiddle",
            Self::Test => "ai.test",
        }
    }

    pub const fn system_prompt(self) -> &'static str {
        match self {
            Self::Instruct => "You are a highly skilled coding assistant and senior software engineer. Your task is to provide concise, accurate, and efficient solutions to the user's coding requests. Please respond with only the revised code. Ensure your answer is in plain text without any Markdown formatting. Focus on best practices, code optimization, and maintainability in your solutions.",
            Self::Document => "Document the provided code using the best practices for documenting code for this language. The answer should be in plain text without Markdown formatting.",
            Self::Fix => "Analyze the provided code snippet, identify any bugs or errors, and provide a corrected version that retains the same functionality. The corrected code should be functional, efficient, and adhere to best programming practices. Return the revised code in plain text without Markdown formatting.",
            Self::Optimize => "Analyze the provided code snippet and propose optimizations to enhance performance. Concentrate on improving efficiency, speed, and resource utilization without altering the core functionality. Present the optimized code in plain text format, excluding any Markdown formatting. Provide only the revised code implementation.",
            Self::Suggest => "Add todo comments to the provided code snippet. The todo comments are to be added to parts of the code that can be improved or fixed. Each the todo comment should explain what needs to be done and give a short explanation of why the change should be made. The answer should be in plain text without Markdown formatting.",
            Self::FillInMiddle => "",
            Self::Test => "This is only a test.",
        }
    }

    pub async fn execute(self, context: Option<String>) -> Option<String> {
        let embedded_instructions = context.as_ref().map(|c| parse_context(c));
        match self {
            Self::Test => context,
            Self::FillInMiddle => {
                let executor = Executor::Completion {
                    model: None,
                    temperature: None,
                    max_tokens: None,
                    top_p: None,
                    context,
                };

                let model = executor.clone().get_model();
                let prompt = executor.clone().get_prompt();

                executor.execute(model, prompt).await.map_or_else(
                    |e| {
                        error!(target: "acai", "Unkown code action: {:?}", self.identifier());
                        error!(target: "acai", "Bad response {e}");
                        None
                    },
                    |response| {
                        response.map_or_else(|| {
                    error!(target: "acai", "Error running code action {:?}", self.identifier());
                    None}, Some)
                    },
                )
            }
            _ => {
                let executor = Executor::ChatCompletion {
                    model: embedded_instructions
                        .as_ref()
                        .and_then(|ei| ei.model.clone()),
                    system_prompt: self.system_prompt().to_string(),
                    temperature: embedded_instructions.as_ref().and_then(|ei| ei.temperature),
                    max_tokens: None,
                    top_p: None,
                    prompt: embedded_instructions
                        .as_ref()
                        .and_then(|ei| ei.prompt.clone()),
                    context: embedded_instructions
                        .as_ref()
                        .map_or(context, |ei| Some(ei.context.clone())),
                };

                let model = executor.clone().get_model();
                let prompt = executor.clone().get_prompt();

                executor.execute(model, prompt).await.map_or_else(
                    |e| {
                        error!(target: "acai", "Unkown code action: {:?}", self.identifier());
                        error!(target: "acai", "Bad response {e}");
                        None
                    },
                    |response| {
                        response.map_or_else(|| {
                    error!(target: "acai", "Error running code action {:?}", self.identifier());
                    None
                }, Some)
                    },
                )
            }
        }
    }

    /// Returns all the commands that the server currently supports.
    pub const fn all() -> [Self; 7] {
        [
            Self::Instruct,
            Self::Document,
            Self::Fix,
            Self::Optimize,
            Self::Suggest,
            Self::FillInMiddle,
            Self::Test,
        ]
    }
}

impl FromStr for AiCodeAction {
    type Err = anyhow::Error;

    fn from_str(name: &str) -> anyhow::Result<Self, Self::Err> {
        Ok(match name {
            "ai.instruct" => Self::Instruct,
            "ai.document" => Self::Document,
            "ai.fix" => Self::Fix,
            "ai.optimize" => Self::Optimize,
            "ai.suggest" => Self::Suggest,
            "ai.fillInMiddle" => Self::FillInMiddle,
            "ai.test" => Self::Test,
            _ => return Err(anyhow::anyhow!("Invalid command `{name}`")),
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CodeActionData {
    pub id: String,
    pub document_uri: Url,
    pub range: Range,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone)]
enum Executor {
    ChatCompletion {
        /// Sets the model to use
        model: Option<String>,

        system_prompt: String,

        /// Sets the temperature value
        temperature: Option<f32>,

        /// Sets the max tokens value
        max_tokens: Option<u32>,

        /// Sets the top-p value
        top_p: Option<f32>,

        /// Sets the prompt
        prompt: Option<String>,

        /// Sets the context
        context: Option<String>,
    },
    Completion {
        /// Sets the model to use
        model: Option<String>,

        /// Sets the temperature value
        temperature: Option<f32>,

        /// Sets the max tokens value
        max_tokens: Option<u32>,

        /// Sets the top-p value
        top_p: Option<f32>,

        /// Sets the context
        context: Option<String>,
    },
}

impl Executor {
    fn get_model(self) -> ModelConfig {
        match self {
            Self::ChatCompletion {
                model,
                system_prompt: _,
                temperature: _,
                max_tokens: _,
                top_p: _,
                prompt: _,
                context: _,
            } => ModelConfig::get_or_default(
                model.unwrap_or_default().as_str(),
                (Provider::Anthropic, "sonnet"),
            ),
            Self::Completion {
                model,
                temperature: _,
                max_tokens: _,
                top_p: _,
                context: _,
            } => ModelConfig::get_or_default(
                model.unwrap_or_default().as_str(),
                (Provider::Mistral, "codestral"),
            ),
        }
    }

    fn get_prompt(self) -> Option<String> {
        match self {
            Self::ChatCompletion {
                model: _,
                system_prompt: _,
                temperature: _,
                max_tokens: _,
                top_p: _,
                prompt,
                context,
            } => {
                let prompt_builder = crate::prompts::Builder::new(&None);

                prompt_builder.map_or(None, |builder| {
                    let mut builder = builder;
                    if let Some(prompt) = &prompt {
                        builder.add_variable("prompt".to_string(), prompt.to_string());
                    }
                    if let Some(context) = &context {
                        builder.add_variable("context".to_string(), context.to_string());
                    }

                    if builder.contains_variables() {
                        builder.build().ok()
                    } else {
                        None
                    }
                })
            }
            Self::Completion {
                model: _,
                temperature: _,
                max_tokens: _,
                top_p: _,
                context,
            } => context,
        }
    }

    async fn execute(
        self,
        model_provider: ModelConfig,
        final_prompt: Option<String>,
    ) -> anyhow::Result<Option<String>> {
        match self {
            Self::ChatCompletion {
                model: _,
                system_prompt,
                temperature,
                max_tokens,
                top_p,
                prompt: _,
                context: _,
            } => {
                let provider = model_provider.provider;
                let model = model_provider.model;

                let mut client = ChatCompletion::new(provider, model, &system_prompt)
                    .temperature(temperature)
                    .top_p(top_p)
                    .max_tokens(max_tokens);

                match final_prompt {
                    Some(content) => {
                        let msg = Message {
                            role: Role::User,
                            content,
                        };

                        let response = (client.send_message(msg).await).map_or(None, |it| it);

                        DataDir::global().save_messages(&client.get_message_history());

                        Ok(response.map(|r| r.content))
                    }
                    _ => Ok(None),
                }
            }
            Self::Completion {
                model: _,
                temperature,
                max_tokens,
                top_p,
                context: _,
            } => {
                let mut client =
                    CompletionClient::new(model_provider.provider, model_provider.model)
                        .temperature(temperature)
                        .top_p(top_p)
                        .max_tokens(max_tokens);

                if let Some(prompt) = final_prompt {
                    let (prefix, suffix) = prompt.find("<fim>").map_or_else(
                        || (prompt.to_string(), None),
                        |index| {
                            let (before, after) = prompt.split_at(index);
                            (before.to_string(), Some(after[5..].to_string()))
                        },
                    );

                    let response =
                        (client.send_message(&prefix, suffix.clone()).await).map_or(None, |it| it);

                    let result = if let Some(msg) = response {
                        // handle FIM response
                        if let Some(sfx) = suffix {
                            Some(format!("{}{}{}", prefix, msg.content, sfx))
                        } else {
                            // Completion response
                            Some(msg.content)
                        }
                    } else {
                        None
                    };

                    DataDir::global().save_messages(&client.get_message_history());

                    Ok(result)
                } else {
                    Ok(None)
                }
            }
        }
    }
}
