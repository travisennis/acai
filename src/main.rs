mod cli;
mod clients;
mod config;
mod files;
mod logger;
mod lsp;
mod models;
mod prompts;
mod search;

use std::error::Error;

use crate::cli::CmdRunner;
use clap::Parser;
use clap::Subcommand;
use cli::chat;
use cli::instruct;
use cli::lsp as lsp_cmd;
use cli::prompt_generator;
use config::DataDir;
use config::DATA_DIR_INSTANCE;
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
    Chat(chat::Cmd),
    Instruct(instruct::Cmd),
    GeneratePrompt(prompt_generator::Cmd),
    Lsp(lsp_cmd::Cmd),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let data_dir = DataDir::new()?;

    match DATA_DIR_INSTANCE.set(data_dir.clone()) {
        Ok(()) => info!(
            "data dir set: {}",
            data_dir.clone().get_cache_dir().display()
        ),
        Err(_) => panic!("data dir could not be set"),
    };

    let _ = logger::configure(&data_dir.get_cache_dir());

    let args = CodingAssistant::parse();

    match args.cmd {
        CodingAssistantCmd::Chat(chat_cmd) => chat_cmd.run().await?,
        CodingAssistantCmd::Instruct(instruct_cmd) => instruct_cmd.run().await?,
        CodingAssistantCmd::GeneratePrompt(prompt_generator_cmd) => {
            prompt_generator_cmd.run().await?;
        }
        CodingAssistantCmd::Lsp(lsp_cmd) => lsp_cmd.run().await?,
    };

    Ok(())
}
