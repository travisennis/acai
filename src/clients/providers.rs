use std::fmt;

use serde::{Deserialize, Serialize};

pub enum Provider {
    Anthropic,
    OpenAI,
    Mistral,
    Google,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum Model {
    #[serde(rename = "gpt-4o")]
    GPT4o,
    #[serde(rename = "gpt-4-turbo-preview")]
    GPT4Turbo,
    #[serde(rename = "gpt-3-turbo")]
    GPT3Turbo,
    #[serde(rename = "claude-3-5-sonnet-20240620")]
    Claude3_5Sonnet,
    #[serde(rename = "claude-3-opus-20240229")]
    Claude3Opus,
    #[serde(rename = "claude-3-sonnet-20240229")]
    Claude3Sonnet,
    #[serde(rename = "claude-3-haiku-20240307")]
    Claude3Haiku,
    #[serde(rename = "codestral-latest")]
    Codestral,
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Model::GPT4o => write!(f, "GPT-4o"),
            Model::GPT4Turbo => write!(f, "GPT-4-Turbo"),
            Model::GPT3Turbo => write!(f, "GPT-3-Turbo"),
            Model::Claude3Opus => write!(f, "Claude 3 Opus"),
            Model::Claude3Sonnet => write!(f, "Claude 3 Sonnet"),
            Model::Claude3Haiku => write!(f, "Claude 3 Haiku"),
            Model::Codestral => write!(f, "Codestral"),
            Model::Claude3_5Sonnet => write!(f, "Claude 3.5 Sonnet"),
        }
    }
}
