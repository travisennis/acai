mod cmds;

use clap::{Parser, Subcommand};

use crate::clients::{Model, Provider};

use self::cmds::{chat, instruct, pipe};

/// coding assistant commands
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct CodingAssistant {
    #[command(subcommand)]
    pub cmd: CodingAssistantCmd,
}

#[derive(Clone, Subcommand)]
pub enum CodingAssistantCmd {
    Chat(chat::Cmd),
    Instruct(instruct::Cmd),
    Pipe(pipe::Cmd),
}

pub fn get_provider_model(model: &str) -> (Provider, Model) {
    match model {
        "gpt-4-turbo" => (Provider::OpenAI, Model::GPT4Turbo),
        "gpt-3-turbo" => (Provider::OpenAI, Model::GPT3Turbo),
        "opus" => (Provider::Anthropic, Model::ClaudeOpus),
        "sonnet" => (Provider::Anthropic, Model::ClaudeSonnet),
        "haiku" => (Provider::Anthropic, Model::ClaudeHaiku),
        _ => (Provider::OpenAI, Model::GPT4Turbo),
    }
}
