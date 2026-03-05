//! acai - AI coding assistant CLI

mod cli;
mod clients;
mod config;
mod logger;
mod models;

use crate::cli::CmdRunner;
use clap::Parser;
use clap::Subcommand;
use cli::instruct;
use config::DataDir;
use log::info;

/// coding assistant commands
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct CodingAssistant {
    #[command(subcommand)]
    pub cmd: CodingAssistantCmd,
}

#[derive(Clone, Subcommand)]
enum CodingAssistantCmd {
    Instruct(instruct::Cmd),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let data_dir = DataDir::new()?;

    info!("data dir: {}", data_dir.get_cache_dir().display());

    let _ = logger::configure(&data_dir.get_cache_dir());

    let args = CodingAssistant::parse();

    match args.cmd {
        CodingAssistantCmd::Instruct(instruct_cmd) => instruct_cmd.run(&data_dir).await?,
    }

    Ok(())
}
