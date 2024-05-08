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

    /// Sets the prompt
    #[arg(short, long)]
    prompt: String,

    /// Sets the temperature value
    #[arg(long, default_value_t = 0.0)]
    temperature: f32,

    /// Sets the max_tokens value
    #[arg(long, default_value_t = 1024)]
    max_tokens: u32,

    /// Sets the top_p value
    #[arg(long, default_value_t = 1.0)]
    top_p: f32,
}

impl Cmd {
    pub async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let system_prompt = "You are a helpful coding assistant. Provide the answer and only the answer in the format requested.";

        let provider_model = get_provider_model(&self.model);

        let mut client = LLMClient::new(provider_model.0, provider_model.1, system_prompt);

        let mut messages: Vec<Message> = vec![];

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

        let prompt = self.prompt.to_string();

        let user_prompt = {
            if let Ok(context) = context {
                format!(
                    "{prompt}\n\
                    ```\n\
                    {context}\n\
                    ```"
                )
            } else {
                prompt
            }
        };

        messages.push(Message {
            role: Role::User,
            content: user_prompt,
        });

        let response = client.send_message(&mut messages).await?;

        if let Some(msg) = response {
            println!("{}", msg.content);
        } else {
            eprintln!("{response:?}");
        }

        Ok(())
    }
}
