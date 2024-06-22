use std::error::Error;

use crate::{
    clients::{
        providers::{Model, Provider},
        CompletionClient,
    },
    config::DataDir,
};

pub struct Complete {
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

impl Complete {
    pub async fn send(&self) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        let model_provider = match self.model.clone().unwrap_or("default".to_string()).as_str() {
            "gpt-4-turbo" => (Provider::OpenAI, Model::GPT4Turbo),
            "gpt-3-turbo" => (Provider::OpenAI, Model::GPT3Turbo),
            "opus" => (Provider::Anthropic, Model::ClaudeOpus),
            "sonnet" => (Provider::Anthropic, Model::ClaudeSonnet),
            "haiku" => (Provider::Anthropic, Model::ClaudeHaiku),
            "codestral" => (Provider::Mistral, Model::Codestral),
            _ => (Provider::Mistral, Model::Codestral),
        };

        let mut client = CompletionClient::new(model_provider.0, model_provider.1)
            .temperature(self.temperature)
            .max_tokens(self.max_tokens);

        let prompt = &self.context;

        if let Some(prompt) = prompt {
            let (prefix, suffix) = prompt.find("<fim>").map_or_else(
                || (prompt.to_string(), None),
                |index| {
                    let (before, after) = prompt.split_at(index);
                    (before.to_string(), Some(after[5..].to_string()))
                },
            );

            let response = client.send_message(&prefix, suffix.clone()).await?;

            let result = if let Some(msg) = response {
                if let Some(sfx) = suffix {
                    Some(format!("{}{}{}", prefix, msg.content, sfx))
                } else {
                    Some(format!("{}{}", prefix, msg.content))
                }
            } else {
                None
            };

            DataDir::new().save_messages(&client.get_message_history());

            Ok(result)
        } else {
            Ok(None)
        }
    }
}
