mod cli;
mod clients;
mod errors;
mod macros;
mod messages;

use std::error::Error;

use clap::Parser;
use cli::CodingAssistant;
use cli::CodingAssistantCmd;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // let args = Args::parse();

    let args = CodingAssistant::parse();

    match args.cmd {
        CodingAssistantCmd::Chat(chat_cmd) => chat_cmd.run().await?,
        CodingAssistantCmd::Pipe(pipe_cmd) => pipe_cmd.run().await?,
        CodingAssistantCmd::Instruct(instruct_cmd) => instruct_cmd.run().await?,
    };

    Ok(())
}
