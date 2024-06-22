use std::error::Error;

use anyhow::Result;
use clap::Args;

use crate::{cli::CmdRunner, operations::Complete};

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
        let context = {
            if atty::is(atty::Stream::Stdin) {
                None
            } else {
                match std::io::read_to_string(std::io::stdin()) {
                    Ok(result) => Some(result),
                    Err(_error) => None,
                }
            }
        };

        let complete = Complete {
            model: self.model.clone(),
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            top_p: self.top_p,
            prompt: None,
            context,
        };

        let response = complete.send().await?;

        if let Some(msg) = response {
            println!("{msg}");
        } else {
            eprintln!("{response:?}");
        }

        Ok(())
    }
}
