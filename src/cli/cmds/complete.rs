use std::error::Error;

use anyhow::Result;
use clap::Args;

use crate::{
    cli::CmdRunner,
    clients::{
        providers::{Model, Provider},
        CompletionClient,
    },
    config::DataDir,
    errors::CAError,
};

#[derive(Clone, Args)]
pub struct Cmd {
    /// Sets the model to use
    #[arg(long)]
    pub model: Option<String>,

    /// Sets the temperature value
    #[arg(long)]
    pub temperature: Option<f32>,

    /// Sets the max tokens value
    #[arg(long)]
    pub max_tokens: Option<u32>,

    /// Sets the top-p value
    #[arg(long)]
    pub top_p: Option<f32>,
}

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
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

        let context: Result<String, CAError> = {
            if atty::is(atty::Stream::Stdin) {
                Err(CAError::Input)
            } else {
                match std::io::read_to_string(std::io::stdin()) {
                    Ok(result) => Ok(result),
                    Err(_error) => Err(CAError::Input),
                }
            }
        };

        let prompt = context.unwrap();

        let (prefix, suffix) = if let Some(index) = prompt.find("<fim>") {
            let (before, after) = prompt.split_at(index);
            (before.to_string(), Some(after[5..].to_string()))
        } else {
            (prompt, None)
        };

        let response = client.send_message(&prefix, suffix.clone()).await?;

        if let Some(msg) = response {
            if let Some(sfx) = suffix {
                println!("{}{}{}", prefix, msg.content, sfx);
            } else {
                println!("{}{}", prefix, msg.content);
            }
        } else {
            eprintln!("{response:?}");
        }

        DataDir::new().save_messages(&client.get_message_history());

        Ok(())
    }
}
