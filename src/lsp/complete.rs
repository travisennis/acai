use std::error::Error;

use crate::{
    clients::{
        providers::{ModelConfig, Provider},
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

    /// Sets the context
    pub context: Option<String>,
}

impl Complete {
    pub async fn send(&self) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
        let model_provider = ModelConfig::get_or_default(
            self.model.clone().unwrap_or_default().as_str(),
            (Provider::Mistral, "codestral"),
        );

        let mut client = CompletionClient::new(model_provider.provider, model_provider.model)
            .temperature(self.temperature)
            .top_p(self.top_p)
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
