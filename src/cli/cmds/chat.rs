use std::{error::Error, path::PathBuf};

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
    files::{get_content_blocks, get_files, parse_patterns},
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

    // Path to the codebase directory
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Patterns to include
    #[clap(long)]
    pub include: Option<String>,

    /// Patterns to exclude
    #[clap(long)]
    pub exclude: Option<String>,

    // Path to a handlebars template
    #[clap(long)]
    pub template: Option<PathBuf>,
}

const SYSTEM_PROMPT: &str = "You are acai, an AI assistant. You specialize in software development with a goal of providing useful guidance to the software developer prompt you. Provide answers in markdown format unless instructed otherwise. If the request is ambiguous or you need more information, ask questions. If you don't know the answer, admit you don't.";

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let model = self.model.clone().unwrap_or_default();
        let config = ModelConfig::get_or_default(&model, (Provider::Anthropic, "sonnet"));

        let mut client = ChatCompletion::new(config.provider, config.model, SYSTEM_PROMPT)
            .temperature(self.temperature)
            .top_p(self.top_p)
            .max_tokens(self.max_tokens);

        // Parse Patterns
        let include_patterns = parse_patterns(&self.include);
        let exclude_patterns = parse_patterns(&self.exclude);

        let file_objects = get_files(
            self.path.clone().unwrap().as_path(),
            &include_patterns,
            &exclude_patterns,
        );

        let code_blocks = get_content_blocks(&file_objects.unwrap());

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
                Ok(line) if line.trim() == "/save" => {
                    DataDir::global().save_messages(&client.get_message_history());
                    println!("\n");
                    skin.print_text("done");
                    println!("\n");
                    continue;
                }
                Ok(line) if line.trim() == "/reset" => {
                    DataDir::global().save_messages(&client.get_message_history());
                    client.clear_message_history();
                    println!("\n");
                    skin.print_text("done");
                    println!("\n");
                    continue;
                }
                Ok(line) if line.starts_with("/model") => {
                    let chosen_model = line.trim_start_matches("/model ").trim().to_string();

                    let config =
                        ModelConfig::get_or_default(&chosen_model, (Provider::Anthropic, "sonnet"));

                    client =
                        ChatCompletion::new(config.provider, config.model.clone(), SYSTEM_PROMPT)
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

                        prompt_builder.add_vec_variable("files".to_string(), &code_blocks);
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
