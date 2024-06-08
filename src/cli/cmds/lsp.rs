use std::error::Error;

use anyhow::Result;
use clap::Args;

use crate::cli::CmdRunner;
use crate::lsp;

#[derive(Clone, Args)]
pub struct Cmd {}

impl CmdRunner for Cmd {
    async fn run(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        lsp::run().await;
        Ok(())
    }
}
