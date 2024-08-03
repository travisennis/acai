use std::{error::Error, path::PathBuf};

use anyhow::Result;
use clap::{arg, Args};

use crate::{
    cli::CmdRunner,
    clients::{
        providers::{ModelConfig, Provider},
        ChatCompletion,
    },
    config::DataDir,
    files::{get_content_blocks, get_file_info, parse_patterns},
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

    /// Sets the prompt
    #[arg(short, long)]
    prompt: Option<String>,
}

const SYSTEM_PROMPT: &str = "You are acai, an AI assistant. You specialize in software development with a goal of providing useful guidance to the software developer prompting you. Provide answers in markdown format unless instructed otherwise. If the request is ambiguous or you need more information, ask questions. If you don't know the answer, admit you don't.";

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let context: Option<String> = {
            if atty::is(atty::Stream::Stdin) {
                None
            } else {
                match std::io::read_to_string(std::io::stdin()) {
                    Ok(result) => Some(result),
                    Err(_error) => None,
                }
            }
        };

        // Parse Patterns
        let include_patterns = parse_patterns(&self.include);
        let exclude_patterns = parse_patterns(&self.exclude);

        let content_blocks = self.path.as_ref().and_then(|path| {
            let file_objects = get_file_info(path.as_path(), &include_patterns, &exclude_patterns);

            file_objects.map_or(None, |files| Some(get_content_blocks(&files)))
        });

        let mut prompt_builder = crate::prompts::Builder::new(&self.template)?;

        if let Some(files) = &content_blocks {
            prompt_builder.add_vec_variable("files".to_string(), files);
        }
        if let Some(prompt) = &self.prompt {
            prompt_builder.add_variable("prompt".to_string(), prompt.to_string());
        }
        if let Some(context) = &context {
            prompt_builder.add_variable("context".to_string(), context.to_string());
        }

        let model_provider = ModelConfig::get_or_default(
            self.model.clone().unwrap_or_default().as_str(),
            (Provider::Anthropic, "sonnet"),
        );

        let provider = model_provider.provider;
        let model = model_provider.model;

        let mut client = ChatCompletion::new(provider, model, SYSTEM_PROMPT)
            .temperature(self.temperature)
            .top_p(self.top_p)
            .max_tokens(self.max_tokens);

        if prompt_builder.contains_variables() {
            let msg = Message {
                role: Role::User,
                content: prompt_builder.build()?,
            };

            let response = client.send_message(msg).await?;

            DataDir::global().save_messages(&client.get_message_history());

            if let Some(response_msg) = response {
                println!("{}", response_msg.content);
            } else {
                eprintln!("{response:?}");
            }
        }

        Ok(())
    }
}
