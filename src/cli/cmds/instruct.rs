use std::error::Error;

use anyhow::Result;
use clap::Args;

use crate::{cli::CmdRunner, operations::Instruct};

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

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        // let system_prompt = "You are a helpful coding assistant. Provide the answer and only the answer in the format requested.";

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

        let op = Instruct {
            model: self.model.clone(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            top_p: self.top_p,
            prompt: self.prompt.clone(),
            context,
        };

        let response = op.send().await?;

        if let Some(response_msg) = response {
            println!("{}", response_msg.content);
        } else {
            eprintln!("{response:?}");
        }

        Ok(())
    }
}
