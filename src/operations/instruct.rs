use std::error::Error;

use crate::{
    clients::{
        providers::{ModelConfig, Provider},
        ChatCompletion,
    },
    config::DataDir,
    models::{Message, Role},
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

    /// Sets the context
    pub context: Option<String>,
}

const DEFAULT_PROMPT: &str = "You are a highly skilled coding assistant and senior software engineer. Your task is to provide concise, accurate, and efficient solutions to the user's coding requests. The user's request will be presented as a TODO comment within the code snippet. Please respond with only the revised code, removing the TODO comment. Ensure your answer is in plain text without any Markdown formatting. Focus on best practices, code optimization, and maintainability in your solutions.";

impl Instruct {
    pub async fn send(&self) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        let system_prompt = DEFAULT_PROMPT;

        let model_provider = ModelConfig::get_or_default(
            self.model.clone().unwrap_or_default().as_str(),
            (Provider::Anthropic, "sonnet"),
        );

        let provider = model_provider.provider;
        let model = model_provider.model;

        let mut client = ChatCompletion::new(provider, model, system_prompt)
            .temperature(self.temperature)
            .top_p(self.top_p)
            .max_tokens(self.max_tokens);

        let mut prompt_builder = crate::prompts::Builder::new(&None)?;

        if let Some(prompt) = &self.prompt {
            prompt_builder.add_variable("prompt".to_string(), prompt.to_string());
        }
        if let Some(context) = &self.context {
            prompt_builder.add_variable("context".to_string(), context.to_string());
        }

        if prompt_builder.contains_variables() {
            let content = prompt_builder.build()?;

            let msg = Message {
                role: Role::User,
                content,
            };

            let response = client.send_message(msg).await?;

            DataDir::global().save_messages(&client.get_message_history());

            return Ok(response);
        }

        Ok(None)
    }
}
