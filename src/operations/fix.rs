use std::error::Error;

use crate::{
    clients::{
        providers::{ModelConfig, Provider},
        ChatCompletionClient,
    },
    config::DataDir,
    models::{Message, Role},
};

pub struct Fix {
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

const DEFAULT_PROMPT: &str = "Your task is to analyze the provided code snippet, identify any bugs or errors present, and provide a corrected version of the code that resolves these issues while retaining the same functionality. The corrected code should be functional, efficient, and adhere to best practices in programming. The answer should be in plain text without Markdown formatting.Only return the revised code.";

impl Fix {
    pub async fn send(&self) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        let system_prompt = DEFAULT_PROMPT;

        let model_provider = ModelConfig::get_or_default(
            self.model.clone().unwrap_or_default().as_str(),
            (Provider::Anthropic, "sonnet"),
        );

        let provider = model_provider.provider;
        let model = model_provider.model;

        let mut client = ChatCompletionClient::new(provider, model, system_prompt)
            .temperature(self.temperature)
            .top_p(self.top_p)
            .max_tokens(self.max_tokens);

        let mut prompt_builder = crate::prompts::Builder::new()?;

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
