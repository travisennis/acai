use clap::{Parser, Subcommand};

use super::cmds::{chat, instruct, pipe};

/// coding assistant commands
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct CodingAssistant {
    #[command(subcommand)]
    pub cmd: CodingAssistantCmd,

    /// Sets the model to use
    #[arg(long, default_value_t = String::from("gpt-4o"))]
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
