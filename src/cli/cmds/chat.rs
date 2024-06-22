use std::{collections::HashMap, error::Error};

use anyhow::Result;
use clap::Args;
use rustyline::{error::ReadlineError, DefaultEditor};
use termimad::MadSkin;

use crate::{
    cli::CmdRunner,
    clients::{
        providers::{Model, Provider},
        ChatCompletionClient,
    },
    config::DataDir,
    errors::CAError,
    models::{Message, Role},
    prompts::PromptBuilder,
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

        let model = self.model.clone().map_or("default".to_string(), |m| m);
        let model_provider = match model.as_str() {
            "gpt-4-turbo" => (Provider::OpenAI, Model::GPT4Turbo),
            "gpt-3-turbo" => (Provider::OpenAI, Model::GPT3Turbo),
            "sonnet" => (Provider::Anthropic, Model::Claude3_5Sonnet),
            "opus3" => (Provider::Anthropic, Model::Claude3Opus),
            "sonnet3" => (Provider::Anthropic, Model::Claude3Sonnet),
            "haiku3" => (Provider::Anthropic, Model::Claude3Haiku),
            "codestral" => (Provider::Mistral, Model::Codestral),
            _ => (Provider::OpenAI, Model::GPT4o),
        };

        let mut client =
            ChatCompletionClient::new(model_provider.0, model_provider.1, system_prompt)
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

                        if let Ok(ref context) = context {
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
