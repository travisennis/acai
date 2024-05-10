use core::fmt;
use std::{env, error::Error};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::messages::{Message, Role};

/// Define a trait named `Response`.
pub trait Response {
    /// Define a method `get_message` that returns an optional `Message`.
    fn get_message(&self) -> Option<Message>;
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AnthropicResponse {
    pub role: Role,
    pub content: Vec<Content>,
}

impl Response for AnthropicResponse {
    fn get_message(&self) -> Option<Message> {
        if let Some(content) = self.content.first() {
            let msg = Message {
                role: self.role,
                content: content.text.to_string(),
            };
            return Some(msg);
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Content {
    text: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct OpenAIResponse {
    pub choices: Vec<Choice>,
}

impl Response for OpenAIResponse {
    fn get_message(&self) -> Option<Message> {
        if let Some(choice) = self.choices.first() {
            let msg = choice.message.clone();
            return Some(msg);
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Choice {
    pub message: Message,
}

pub enum Provider {
    Anthropic,
    OpenAI,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum Model {
    #[serde(rename = "gpt-4-turbo-preview")]
    GPT4Turbo,
    #[serde(rename = "gpt-3-turbo")]
    GPT3Turbo,
    #[serde(rename = "claude-3-opus-20240229")]
    ClaudeOpus,
    #[serde(rename = "claude-3-sonnet-20240229")]
    ClaudeSonnet,
    #[serde(rename = "claude-3-haiku-20240307")]
    ClaudeHaiku,
}

// impl Into<String> for Model {
//     fn into(self) -> String {
//         match self {
//             Model::GPT4Turbo => "gpt-4-turbo-preview".to_string(),
//             Model::GPT3Turbo => "gpt-3-turbo".to_string(),
//             Model::ClaudeOpus => "claude-3-opus-20240229".to_string(),
//             Model::ClaudeSonnet => "claude-3-sonnet-20240229".to_string(),
//             Model::ClaudeHaiku => "claude-3-haiku-20240307".to_string(),
//         }
//     }
// }

// impl_enum_string_serialization!(
//     Model,
//     GPT4Turbo => "gpt-4-turbo-preview",
//     GPT3Turbo => "gpt-3-turbo",
//     ClaudeOpus => "claude-3-opus-20240229",
//     ClaudeSonnet => "claude-3-sonnet-20240229",
//     ClaudeHaiku => "claude-3-haiku-20240307"
// );

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Model::GPT4Turbo => write!(f, "GPT-4-Turbo"),
            Model::GPT3Turbo => write!(f, "GPT-3-Turbo"),
            Model::ClaudeOpus => write!(f, "Claude Opus"),
            Model::ClaudeSonnet => write!(f, "Claude Sonnet"),
            Model::ClaudeHaiku => write!(f, "Claude Haiku"),
        }
    }
}

pub struct LLMClient {
    provider: Provider,
    model: Model,
    token: String,
    temperature: f32,
    top_p: f32,
    max_tokens: u32,
    system: String,
    messages: Vec<Message>,
}

impl LLMClient {
    pub fn new(
        provider: Provider,
        model: Model,
        temperature: f32,
        top_p: f32,
        max_tokens: u32,
        system_prompt: &str,
    ) -> Self {
        let token = match provider {
            Provider::Anthropic => env::var("CLAUDE_API_KEY"),
            Provider::OpenAI => env::var("OPENAI_API_KEY"),
        }
        .unwrap_or_else(|_error| panic!("Error: Environment variable not set."));

        let msgs: Vec<Message> = match provider {
            Provider::Anthropic => vec![],
            Provider::OpenAI => vec![Message {
                role: Role::System,
                content: system_prompt.to_string(),
            }],
        };

        LLMClient {
            provider,
            model,
            token,
            temperature,
            max_tokens,
            top_p,
            system: system_prompt.to_string(),
            messages: msgs,
        }
    }

    pub async fn send_message(
        &mut self,
        messages: &mut Vec<Message>,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        self.messages.append(messages);

        let prompt = match &self.provider {
            Provider::Anthropic => json!({
                "model": self.model,
                "temperature": self.temperature,
                "max_tokens": self.max_tokens,
                "top_p": self.top_p,
                "system": self.system,
                "messages": self.messages
            }),
            Provider::OpenAI => json!({
                "model": self.model,
                "temperature": self.temperature,
                "top_p": self.top_p,
                "max_tokens": self.max_tokens,
                "messages": self.messages
            }),
        };

        let request_url = match &self.provider {
            Provider::Anthropic => "https://api.anthropic.com/v1/messages",
            Provider::OpenAI => "https://api.openai.com/v1/chat/completions",
        };

        let req_base = Client::new()
            .post(request_url)
            .json(&prompt)
            .header("content-type", "application/json");
        let req = match &self.provider {
            Provider::Anthropic => req_base
                .header("anthropic-version", "2023-06-01")
                .header("x-api-key", self.token.to_string()),
            Provider::OpenAI => req_base.bearer_auth(self.token.to_string()),
        };

        let response = req.send().await?;

        if response.status().is_success() {
            let message = match &self.provider {
                Provider::Anthropic => {
                    let anth_response = response.json::<AnthropicResponse>().await?;
                    anth_response.get_message()
                }
                Provider::OpenAI => {
                    let ai_response = response.json::<OpenAIResponse>().await?;
                    ai_response.get_message()
                }
            };
            Ok(message)
        } else {
            match response.json::<Value>().await {
                Ok(resp_json) => match serde_json::to_string_pretty(&resp_json) {
                    Ok(resp_formatted) => {
                        Err(format!("{}\n\n{}", self.model, resp_formatted).into())
                    }
                    Err(e) => Err(format!("Failed to format response JSON: {e}").into()),
                },
                Err(e) => Err(format!("Failed to parse response JSON: {e}").into()),
            }
        }
    }

    pub fn get_message_history(&self) -> Vec<Message> {
        let mut msgs = self.messages.clone();
        match self.provider {
            Provider::Anthropic => {
                let mut result = vec![Message {
                    role: Role::System,
                    content: self.system.to_string(),
                }];
                result.append(&mut msgs);
                result
            }
            Provider::OpenAI => msgs,
        }
    }
}
