use std::error::Error;

use anyhow::Result;
use clap::Args;

use crate::{
    cli::get_provider_model,
    clients::LLMClient,
    errors::CAError,
    messages::{Message, Role},
};

#[derive(Clone, Args)]
pub struct Cmd {
    /// Sets the model to use
    #[arg(long, default_value_t = String::from("gpt-4-turbo"))]
    model: String,

    /// Sets the temperature value
    #[arg(short, long, default_value_t = 0.2)]
    temperature: f32,

    /// Sets the stdin prompt
    std_prompt: Vec<String>,
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let system_prompt = "You are a helpful coding assistant. Provide the answer and only the answer. The answer should be in plain text without Markdown formatting.";

        let context: Result<String, CAError> = {
            if atty::is(atty::Stream::Stdin) {
                Err(CAError::Input)
            } else {
                Ok(std::io::read_to_string(std::io::stdin()).unwrap())
            }
        };

        let provider_model = get_provider_model(&self.model);

        let mut client = LLMClient::new(provider_model.0, provider_model.1, system_prompt);

        let mut messages: Vec<Message> = vec![];

        if let Ok(context) = context {
            messages.push(Message {
                role: Role::User,
                content: context,
            });
        };

        let prompt: Result<String, CAError> = {
            if self.std_prompt.is_empty() {
                Err(CAError::Input)
            } else {
                Ok(self.std_prompt.join(" "))
            }
        };

        if let Ok(prompt) = prompt {
            messages.push(Message {
                role: Role::User,
                content: prompt,
            });
        };

        let response = client.send_message(&mut messages).await?;

        if let Some(msg) = response {
            println!("{}", msg.content);
        } else {
            eprintln!("{response:?}");
            panic!("Did not receive a valid response.");
        }
        Ok(())
    }
}
