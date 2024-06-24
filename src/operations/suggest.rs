use std::{collections::HashMap, error::Error};

use crate::{
    clients::{
        providers::{Model, Provider},
        ChatCompletionClient,
    },
    config::DataDir,
    models::{Message, Role},
    prompts::PromptBuilder,
};

pub struct Suggest {
    /// Sets the model to use
    pub model: Option<String>,

    /// Sets the temperature value
    pub temperature: Option<f32>,

    /// Sets the max tokens value
    pub max_tokens: Option<u32>,

    /// Sets the top-p value
    pub top_p: Option<f32>,

    /// Sets the prompt
    pub prompt: Option<String>,

    /// Sets the context
    pub context: Option<String>,
}

const DEFAULT_PROMPT: &str = "Your task is to add todo comments to the provided code snippet. The todo comments are to be added to parts of the code that can be improved or fixed. The todo comment should explain what needs to be done and give a short explanation of why.";

impl Suggest {
    pub async fn send(&self) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        let system_prompt = DEFAULT_PROMPT;

        let model_provider = self
            .model
            .clone()
            .map_or((Provider::OpenAI, Model::GPT4o), |model| {
                match model.as_str() {
                    "gpt-4-turbo" => (Provider::OpenAI, Model::GPT4Turbo),
                    "gpt-3-turbo" => (Provider::OpenAI, Model::GPT3Turbo),
                    "sonnet" => (Provider::Anthropic, Model::Claude3_5Sonnet),
                    "opus3" => (Provider::Anthropic, Model::Claude3Opus),
                    "sonnet3" => (Provider::Anthropic, Model::Claude3Sonnet),
                    "haiku3" => (Provider::Anthropic, Model::Claude3Haiku),
                    "codestral" => (Provider::Mistral, Model::Codestral),
                    _ => (Provider::OpenAI, Model::GPT4o),
                }
            });

        let provider = model_provider.0;
        let model = model_provider.1;

        let mut client = ChatCompletionClient::new(provider, model, system_prompt)
            .temperature(self.temperature)
            .top_p(self.top_p)
            .max_tokens(self.max_tokens);

        let prompt_builder = PromptBuilder::new()?;

        let mut data = HashMap::new();

        if let Some(prompt) = &self.prompt {
            data.insert("prompt".to_string(), prompt.to_string());
        }
        if let Some(context) = &self.context {
            data.insert("context".to_string(), context.to_string());
        }

        if !data.is_empty() {
            let msg = Message {
                role: Role::User,
                content: prompt_builder.build(&data)?,
            };

            let response = client.send_message(msg).await?;

            DataDir::new().save_messages(&client.get_message_history());

            return Ok(response);
        }

        Ok(None)
    }
}
