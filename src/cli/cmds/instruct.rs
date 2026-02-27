use std::error::Error;

use anyhow::Result;
use clap::Args;

use crate::{
    cli::CmdRunner,
    clients::{
        providers::{ModelConfig, Provider},
        ChatCompletion,
    },
    config::DataDir,
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

    /// Sets the prompt
    #[arg(short, long)]
    prompt: Option<String>,
}

const SYSTEM_PROMPT: &str = "You are acai, an AI coding assistant. You specialize in helping software developers with the tasks that help them write better software. Pay close attention to the instructions given to you by the user and always follow those instructions. Return your reponse as markdown unless the user indicates a different return format. It is very important that you format your response according to the user instructions as that formatting will be used to accomplish specific tasks.";

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let context: Option<String> = {
            if atty::is(atty::Stream::Stdin) {
                None
            } else {
                std::io::read_to_string(std::io::stdin()).ok()
            }
        };

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

        // Build content from prompt and optional stdin context
        let content = match (&self.prompt, &context) {
            (Some(prompt), Some(ctx)) => format!("{}\n\n{}", prompt, ctx),
            (Some(prompt), None) => prompt.clone(),
            (None, Some(ctx)) => ctx.clone(),
            (None, None) => String::new(),
        };

        let msg = Message {
            role: Role::User,
            content,
        };

        let response = client.send_message(msg).await?;

        DataDir::global().save_messages(&client.get_message_history());

        if let Some(response_msg) = response {
            println!("{}", response_msg.content);
        } else {
            eprintln!("{response:?}");
        }

        Ok(())
    }
}
