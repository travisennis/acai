use std::error::Error;

use crate::{
    clients::{
        providers::{ModelConfig, Provider},
        ChatCompletion,
    },
    config::DataDir,
    models::{Message, Role},
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

const DEFAULT_PROMPT: &str = "Add todo comments to the provided code snippet. The todo comments are to be added to parts of the code that can be improved or fixed. Each the todo comment should explain what needs to be done and give a short explanation of why the change should be made. The answer should be in plain text without Markdown formatting.";

impl Suggest {
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
            let msg = Message {
                role: Role::User,
                content: prompt_builder.build()?,
            };

            let response = client.send_message(msg).await?;

            DataDir::global().save_messages(&client.get_message_history());

            return Ok(response);
        }

        Ok(None)
    }
}
