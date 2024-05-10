use std::error::Error;

use clap::{Parser, Subcommand};

use crate::clients::{Model, Provider};

use super::cmds::{chat, instruct, pipe};

/// coding assistant commands
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct CodingAssistant {
    #[command(subcommand)]
    pub cmd: CodingAssistantCmd,

    /// Sets the model to use
    #[arg(long, default_value_t = String::from("gpt-4-turbo"))]
    pub model: String,

    /// Sets the temperature value
    #[arg(long, default_value_t = 0.0)]
    pub temperature: f32,

    /// Sets the max tokens value
    #[arg(long, default_value_t = 1024)]
    pub max_tokens: u32,

    /// Sets the top-p value
    #[arg(long, default_value_t = 1.0)]
    pub top_p: f32,
}

#[derive(Clone, Subcommand)]
pub enum CodingAssistantCmd {
    Chat(chat::Cmd),
    Instruct(instruct::Cmd),
    Pipe(pipe::Cmd),
}

pub struct CmdConfig {
    pub provider: Provider,
    pub model: Model,
    pub context: Option<String>,
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u32,
}

impl CmdConfig {
    pub fn new(
        model: &str,
        context: Option<String>,
        temperature: f32,
        top_p: f32,
        max_tokens: u32,
    ) -> Self {
        let model_provider = match model {
            // "gpt-4-turbo" => (Provider::OpenAI, Model::GPT4Turbo),
            "gpt-3-turbo" => (Provider::OpenAI, Model::GPT3Turbo),
            "opus" => (Provider::Anthropic, Model::ClaudeOpus),
            "sonnet" => (Provider::Anthropic, Model::ClaudeSonnet),
            "haiku" => (Provider::Anthropic, Model::ClaudeHaiku),
            _ => (Provider::OpenAI, Model::GPT4Turbo),
        };

        Self {
            provider: model_provider.0,
            model: model_provider.1,
            context,
            temperature,
            top_p,
            max_tokens,
        }
    }
}

pub trait CmdRunner {
    async fn run(&self, cfg: CmdConfig) -> Result<(), Box<dyn Error + Send + Sync>>;
}
