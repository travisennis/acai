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

pub struct Instruct {
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

    pub context: Option<String>,
}

const DEFAULT_PROMPT: &str = "You are a helpful coding assistant and senior software engineer. Provide the answer and only the answer to the user's request. The user's request will be in a TODO comment within the code snippet.  The answer should be in plain text without Markdown formatting. Only return the revised code and remove the TODO comment.";

impl Instruct {
    pub async fn send(&self) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        let system_prompt = DEFAULT_PROMPT;

        let model_provider = match self.model.clone().unwrap_or("default".to_string()).as_str() {
            "gpt-4-turbo" => (Provider::OpenAI, Model::GPT4Turbo),
            "gpt-3-turbo" => (Provider::OpenAI, Model::GPT3Turbo),
            "opus" => (Provider::Anthropic, Model::ClaudeOpus),
            "sonnet" => (Provider::Anthropic, Model::ClaudeSonnet),
            "haiku" => (Provider::Anthropic, Model::ClaudeHaiku),
            "codestral" => (Provider::Mistral, Model::Codestral),
            _ => (Provider::OpenAI, Model::GPT4o),
        };

        let mut client =
            ChatCompletionClient::new(model_provider.0, model_provider.1, system_prompt)
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
