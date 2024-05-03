mod cli;
mod clients;
mod config;
mod errors;
mod macros;
mod messages;

use std::error::Error;

use clap::Parser;
use cli::CodingAssistant;
use cli::CodingAssistantCmd;
use config::create_data_dir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let args = CodingAssistant::parse();

    create_data_dir();

    match args.cmd {
        CodingAssistantCmd::Chat(chat_cmd) => chat_cmd.run().await?,
        CodingAssistantCmd::Pipe(pipe_cmd) => pipe_cmd.run().await?,
        CodingAssistantCmd::Instruct(instruct_cmd) => instruct_cmd.run().await?,
    };

    Ok(())
}
