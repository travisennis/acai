use anyhow::Result;
use clap::Args;
use log::{error, info};
use std::{env, error::Error, path::PathBuf};

use crate::{
    cli::CmdRunner,
    files::{get_code_blocks, get_files, parse_patterns},
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

    // Path to a handlebars template
    #[clap(long)]
    pub template: Option<PathBuf>,
}

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        match env::current_dir() {
            Ok(path) => info!("The current working directory is: {}", path.display()),
            Err(e) => error!("Error: {e}"),
        }

        // Parse Patterns
        let include_patterns = parse_patterns(&self.include);
        let exclude_patterns = parse_patterns(&self.exclude);

        let file_objects = get_files(
            self.path.clone().unwrap().as_path(),
            &include_patterns,
            &exclude_patterns,
        );

        let code_blocks = get_code_blocks(&file_objects.unwrap());

        let mut prompt_builder = crate::prompts::Builder::new(&self.template)?;

        prompt_builder.add_vec_variable("files".to_string(), &code_blocks);

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
