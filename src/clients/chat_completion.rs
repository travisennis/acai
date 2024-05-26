use core::fmt;
use std::{env, error::Error};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::models::{Message, Role};

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
    #[serde(rename = "gpt-4o")]
    GPT4o,
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

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Model::GPT4o => write!(f, "GPT-4o"),
            Model::GPT4Turbo => write!(f, "GPT-4-Turbo"),
            Model::GPT3Turbo => write!(f, "GPT-3-Turbo"),
            Model::ClaudeOpus => write!(f, "Claude Opus"),
            Model::ClaudeSonnet => write!(f, "Claude Sonnet"),
            Model::ClaudeHaiku => write!(f, "Claude Haiku"),
        }
    }
}

#[allow(clippy::module_name_repetitions)]
pub struct ChatCompletionClient {
    provider: Provider,
    model: Model,
    token: String,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    system: String,
    messages: Vec<Message>,
    stop: Option<Vec<String>>,
    presence_penalty: Option<f32>,
    frequency_penalty: Option<f32>,
    logit_bias: Option<std::collections::HashMap<String, f32>>,
    user: Option<String>,
    top_k: Option<u32>,
    stream: bool,
}

impl ChatCompletionClient {
    pub fn new(provider: Provider, model: Model, system_prompt: &str) -> Self {
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

        ChatCompletionClient {
            provider,
            model,
            token,
            temperature: Some(0.0),
            max_tokens: Some(1028),
            top_p: None,
            system: system_prompt.to_string(),
            messages: msgs,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            logit_bias: None,
            user: None,
            top_k: None,
            stream: false,
        }
    }

    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn top_p(mut self, top_p: Option<f32>) -> Self {
        if let Some(top_p) = top_p {
            self.top_p = Some(top_p);
        }
        self
    }

    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    #[allow(dead_code)]
    pub fn stop(mut self, stop: Vec<String>) -> Self {
        self.stop = Some(stop);
        self
    }

    #[allow(dead_code)]
    pub fn presence_penalty(mut self, presence_penalty: f32) -> Self {
        self.presence_penalty = Some(presence_penalty);
        self
    }

    #[allow(dead_code)]
    pub fn frequency_penalty(mut self, frequency_penalty: f32) -> Self {
        self.frequency_penalty = Some(frequency_penalty);
        self
    }

    #[allow(dead_code)]
    pub fn logit_bias(mut self, logit_bias: std::collections::HashMap<String, f32>) -> Self {
        self.logit_bias = Some(logit_bias);
        self
    }

    #[allow(dead_code)]
    pub fn user(mut self, user: String) -> Self {
        self.user = Some(user);
        self
    }

    #[allow(dead_code)]
    pub fn top_k(mut self, top_k: u32) -> Self {
        self.top_k = Some(top_k);
        self
    }

    #[allow(dead_code)]
    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub async fn send_message(
        &mut self,
        message: Message,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        self.messages.push(message);

        let prompt = match &self.provider {
            Provider::Anthropic => json!({
                "model": self.model,
                "temperature": self.temperature,
                "max_tokens": self.max_tokens,
                "top_p": self.top_p,
                "top_k": self.top_k,
                "stream": self.stream,
                "system": self.system,
                "messages": self.messages
            }),
            Provider::OpenAI => json!({
                "model": self.model,
                "temperature": self.temperature,
                "top_p": self.top_p,
                "max_tokens": self.max_tokens,
                "stream": self.stream,
                "messages": self.messages,
                "presence_penalty": self.presence_penalty,
                "frequency_penalty": self.frequency_penalty,
                "stop": self.stop,
                "logit_bias": self.logit_bias,
                "user": self.user,
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

            if let Some(msg) = message.clone() {
                self.messages.push(msg);
            }

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
