use std::{collections::HashMap, error::Error};

use anyhow::Result;
use clap::Args;

use crate::{
    cli::{CmdConfig, CmdRunner},
    clients::ChatCompletionClient,
    config::DataDir,
    models::{Message, Role},
    prompts::PromptBuilder,
};

#[derive(Clone, Args)]
pub struct Cmd {
    /// Sets the prompt
    #[arg(short, long)]
    prompt: Option<String>,
}

const DEFAULT_PROMPT: &str = "You are a helpful coding assistant and senior software engineer. Provide the answer and only the answer to the user's request. The user's request will be in a TODO comment within the code snippet.  The answer should be in plain text without Markdown formatting. Only return the revised code and remove the TODO comment.";

impl CmdRunner for Cmd {
    async fn run(&self, cfg: CmdConfig) -> Result<(), Box<dyn Error + Send + Sync>> {
        // let system_prompt = "You are a helpful coding assistant. Provide the answer and only the answer in the format requested.";

        let system_prompt = DEFAULT_PROMPT;

        let mut client = ChatCompletionClient::new(cfg.provider, cfg.model, system_prompt)
            .temperature(cfg.temperature)
            .top_p(cfg.top_p)
            .max_tokens(cfg.max_tokens);

        let prompt_builder = PromptBuilder::new()?;

        let mut data = HashMap::new();

        if let Some(prompt) = &self.prompt {
            data.insert("prompt".to_string(), prompt.to_string());
        }
        if let Some(context) = cfg.context {
            data.insert("context".to_string(), context.to_string());
        }

        if !data.is_empty() {
            let msg = Message {
                role: Role::User,
                content: prompt_builder.build(&data)?,
            };

            let response = client.send_message(msg).await?;

            if let Some(response_msg) = response {
                println!("{}", response_msg.content);
            } else {
                eprintln!("{response:?}");
            }

            DataDir::new().save_messages(&client.get_message_history());
        }

        Ok(())
    }
}
