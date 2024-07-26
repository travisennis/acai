use std::error::Error;

use anyhow::Result;
use clap::Args;
use rustyline::{error::ReadlineError, DefaultEditor};
use termimad::MadSkin;

use crate::{
    cli::CmdRunner,
    clients::{
        providers::{ModelConfig, Provider},
        ChatCompletion,
    },
    config::DataDir,
    errors::CAError,
    models::{Message, Role},
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
        let system_prompt = "You are a helpful coding assistant. Provide answers in markdown format unless instructed otherwise. If the request is ambiguous, ask questions. If you don't know the answer, admit you don't.";

        let model = self.model.clone().unwrap_or_default();
        let config = ModelConfig::get_or_default(&model, (Provider::Anthropic, "sonnet"));

        let mut client = ChatCompletion::new(config.provider, config.model, system_prompt)
            .temperature(self.temperature)
            .top_p(self.top_p)
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

        let mut rl = DefaultEditor::new()?;

        let skin = MadSkin::default();

        let mut prompt_builder = crate::prompts::Builder::new(&None)?;

        let mut is_first_iteration = true;

        loop {
            let readline = rl.readline("> ");
            match readline {
                Ok(line) if line.trim() == "/bye" => {
                    break;
                }
                Ok(line) if line.starts_with("/model") => {
                    let chosen_model = line.trim_start_matches("/model ").trim().to_string();

                    let config =
                        ModelConfig::get_or_default(&chosen_model, (Provider::Anthropic, "sonnet"));

                    client = ChatCompletionClient::new(
                        config.provider,
                        config.model.clone(),
                        system_prompt,
                    )
                    .temperature(self.temperature)
                    .top_p(self.top_p)
                    .max_tokens(self.max_tokens);

                    println!("\n");
                    skin.print_text(&format!("Model set to {}", config.model));
                    println!("\n");

                    continue;
                }
                Ok(line) => {
                    prompt_builder.add_variable("prompt".to_string(), line);
                    if is_first_iteration {
                        is_first_iteration = false;

                        if let Ok(ref context) = context {
                            prompt_builder.add_variable("context".to_string(), context.to_string());
                        }
                    }

                    let user_msg = Message {
                        role: Role::User,
                        content: prompt_builder.build()?,
                    };

                    let response = client.send_message(user_msg).await?;

                    if let Some(msg) = response {
                        println!("\n");
                        skin.print_text(&msg.content);
                        println!("\n");
                    }

                    prompt_builder.clear_variables();
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

        DataDir::global().save_messages(&client.get_message_history());

        Ok(())
    }
}
