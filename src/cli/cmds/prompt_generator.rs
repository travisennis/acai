use anyhow::Result;
use std::{env, error::Error, path::PathBuf};

use clap::Args;

use crate::{
    cli::CmdRunner,
    models::{Message, Role},
};

#[derive(Clone, Args)]
pub struct Cmd {
    // Path to the codebase directory
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Patterns to include
    #[clap(long)]
    pub include: Option<String>,

    /// Patterns to exclude
    #[clap(long)]
    pub exclude: Option<String>,
}

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        match env::current_dir() {
            Ok(path) => println!("The current working directory is: {}", path.display()),
            Err(e) => println!("Error: {e}"),
        }

        let mut prompt_builder = crate::prompts::Builder::new()?;

        prompt_builder.add_variable("test".to_string(), "test".to_string());

        if prompt_builder.contains_variables() {
            let msg = Message {
                role: Role::User,
                content: prompt_builder.build()?,
            };

            println!("Final: {}", msg.content);
        }

        Ok(())
    }
}
