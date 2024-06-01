mod cli;
mod clients;
mod config;
mod errors;
mod models;
mod prompts;

use std::error::Error;

use clap::Parser;
use cli::CmdConfig;
use cli::CmdRunner;
use cli::CodingAssistant;
use cli::CodingAssistantCmd;
use config::DataDir;
use errors::CAError;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    DataDir::create();

    let args = CodingAssistant::parse();

    let context: Result<String, CAError> = {
        if atty::is(atty::Stream::Stdin) {
            Err(CAError::Input)
        } else {
            match std::io::read_to_string(std::io::stdin()) {
                Ok(result) => Ok(result),
                Err(_error) => Err(CAError::Input),
            }
        }
    };

    let cfg = CmdConfig::new(
        &args.model,
        context.ok(),
        args.temperature,
        args.top_p,
        args.max_tokens,
    );

    match args.cmd {
        CodingAssistantCmd::Chat(chat_cmd) => chat_cmd.run(cfg).await?,
        CodingAssistantCmd::Pipe(pipe_cmd) => pipe_cmd.run(cfg).await?,
        CodingAssistantCmd::Instruct(instruct_cmd) => instruct_cmd.run(cfg).await?,
        CodingAssistantCmd::PromptGenerator(prompt_generator_cmd) => {
            prompt_generator_cmd.run(cfg).await?;
        }
    };

    Ok(())
}
