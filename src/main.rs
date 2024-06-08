mod cli;
mod clients;
mod config;
mod errors;
mod models;
mod prompts;

use std::error::Error;

use crate::cli::CmdRunner;
use clap::Parser;
use clap::Subcommand;
use cli::chat;
use cli::complete;
use cli::instruct;
use cli::pipe;
use cli::prompt_generator;
use config::DataDir;

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
    Pipe(pipe::Cmd),
    Complete(complete::Cmd),
    PromptGenerator(prompt_generator::Cmd),
    Lsp(lsp_cmd::Cmd),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    DataDir::create();

    let args = CodingAssistant::parse();

    match args.cmd {
        CodingAssistantCmd::Chat(chat_cmd) => chat_cmd.run().await?,
        CodingAssistantCmd::Pipe(pipe_cmd) => pipe_cmd.run().await?,
        CodingAssistantCmd::Instruct(instruct_cmd) => instruct_cmd.run().await?,
        CodingAssistantCmd::Complete(complete_cmd) => complete_cmd.run().await?,
        CodingAssistantCmd::PromptGenerator(prompt_generator_cmd) => {
            prompt_generator_cmd.run().await?;
        }
    };

    Ok(())
}
