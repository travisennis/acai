use std::error::Error;

use anyhow::Result;
use clap::Args;

use crate::{
    cli::{CmdConfig, CmdRunner},
    clients::LLMClient,
    models::{Message, Role},
};

#[derive(Clone, Args)]
pub struct Cmd {
    /// Sets the prompt
    #[arg(short, long)]
    prompt: String,
}

impl CmdRunner for Cmd {
    async fn run(&self, cfg: CmdConfig) -> Result<(), Box<dyn Error + Send + Sync>> {
        let system_prompt = "You are a helpful coding assistant. Provide the answer and only the answer in the format requested.";

        let mut client = LLMClient::new(
            cfg.provider,
            cfg.model,
            cfg.temperature,
            cfg.top_p,
            cfg.max_tokens,
            system_prompt,
        );

        let mut messages: Vec<Message> = vec![];

        let prompt = self.prompt.to_string();

        let user_prompt = {
            if let Some(context) = cfg.context {
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
