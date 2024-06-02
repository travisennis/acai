use std::error::Error;

use anyhow::Result;
use clap::Args;

use crate::{
    cli::{CmdConfig, CmdRunner},
    clients::CompletionClient,
    config::DataDir,
};

#[derive(Clone, Args)]
pub struct Cmd {}

impl CmdRunner for Cmd {
    async fn run(&self, cfg: CmdConfig) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut client = CompletionClient::new(cfg.provider, cfg.model)
            .temperature(cfg.temperature)
            .max_tokens(cfg.max_tokens);

        let prompt = cfg.context.unwrap();

        let response = client.send_message(prompt.as_str()).await?;

        if let Some(msg) = response {
            println!("{}", msg.content);
        } else {
            eprintln!("{response:?}");
        }

        DataDir::new().save_messages(&client.get_message_history());

        Ok(())
    }
}
