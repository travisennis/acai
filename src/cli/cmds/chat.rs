use std::{collections::HashMap, error::Error};

use anyhow::Result;
use clap::Args;
use rustyline::{error::ReadlineError, DefaultEditor};
use termimad::MadSkin;

use crate::{
    cli::{CmdConfig, CmdRunner},
    clients::ChatCompletionClient,
    config::DataDir,
    models::{Message, Role},
    prompts::PromptBuilder,
};

#[derive(Clone, Args)]
pub struct Cmd {}

impl CmdRunner for Cmd {
    async fn run(&self, cfg: CmdConfig) -> Result<(), Box<dyn Error + Send + Sync>> {
        let system_prompt = "You are a helpful coding assistant. Provide answers in markdown format unless instructed otherwise. If the request is ambiguous, ask questions. If you don't know the answer, admit you don't.";

        let mut client = ChatCompletionClient::new(cfg.provider, cfg.model, system_prompt)
            .temperature(cfg.temperature)
            .top_p(cfg.top_p)
            .max_tokens(cfg.max_tokens);

        let mut rl = DefaultEditor::new().expect("Editor not initialized.");

        let skin = MadSkin::default();

        let prompt_builder = PromptBuilder::new()?;

        let mut is_first_iteration = true;

        loop {
            let readline = rl.readline("> ");
            match readline {
                Ok(line) if line.trim() == "bye" => {
                    break;
                }
                Ok(line) => {
                    let mut data = HashMap::new();
                    data.insert("prompt".to_string(), line);
                    if is_first_iteration {
                        is_first_iteration = false;

                        if let Some(ref context) = cfg.context {
                            data.insert("context".to_string(), context.to_string());
                        }
                    }

                    let user_msg = Message {
                        role: Role::User,
                        content: prompt_builder.build(&data)?,
                    };

                    let response = client.send_message(user_msg).await?;

                    if let Some(msg) = response {
                        println!("\n");
                        skin.print_text(&msg.content);
                        println!("\n");
                    }
                }
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => {
                    break;
                }
                Err(err) => {
                    println!("Error: {err:?}");
                    break;
                }
            }
        }

        DataDir::new().save_messages(&client.get_message_history());

        Ok(())
    }
}
