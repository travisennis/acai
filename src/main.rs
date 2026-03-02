mod cli;
mod clients;
mod config;
mod logger;
mod models;

use std::error::Error;

use crate::cli::CmdRunner;
use clap::Parser;
use clap::Subcommand;
use cli::instruct;
use config::DATA_DIR_INSTANCE;
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
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let data_dir = DataDir::new()?;

    match DATA_DIR_INSTANCE.set(data_dir.clone()) {
        Ok(()) => info!(
            "data dir set: {}",
            data_dir.clone().get_cache_dir().display()
        ),
        Err(e) => panic!("data dir could not be set: {e:?}"),
    }

    let _ = logger::configure(&data_dir.get_cache_dir());

    let args = CodingAssistant::parse();

    match args.cmd {
        CodingAssistantCmd::Instruct(instruct_cmd) => instruct_cmd.run().await?,
    }

    Ok(())
}
