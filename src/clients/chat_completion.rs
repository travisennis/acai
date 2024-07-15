use std::{env, error::Error};

use reqwest::Client;
use serde_json::{json, Value};

use crate::models::{IntoMessage, Message, Role};

use super::{
    anthropic::{Request as AnthropicRequest, Response as AnthropicResponse},
    google::{
        Instruction, Part, Request as GoogleRequest, Response as GoogleResponse, SystemInstruction,
    },
    mistral::Response as MistralResponse,
    open_ai::Response as OpenAIResponse,
    providers::Provider,
};

#[allow(clippy::module_name_repetitions)]
pub struct ChatCompletionClient {
    provider: Provider,
    model: String,
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
    pub fn new(provider: Provider, model: String, system_prompt: &str) -> Self {
        let token = match provider {
            Provider::Anthropic => env::var("CLAUDE_API_KEY"),
            Provider::OpenAI => env::var("OPENAI_API_KEY"),
            Provider::Mistral => env::var("MISTRAL_API_KEY"),
            Provider::Google => env::var("GOOGLE_API_KEY"),
            Provider::Ollama => Ok(String::new()),
        }
        .unwrap_or_else(|_error| panic!("Error: Environment variable not set."));

        let msgs: Vec<Message> = match provider {
            Provider::OpenAI | Provider::Mistral | Provider::Ollama => vec![Message {
                role: Role::System,
                content: system_prompt.to_string(),
            }],
            Provider::Google | Provider::Anthropic => vec![],
        };

        Self {
            provider,
            model,
            token,
            temperature: Some(0.0),
            max_tokens: None,
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

    pub const fn temperature(mut self, temperature: Option<f32>) -> Self {
        if let Some(temperature) = temperature {
            self.temperature = Some(temperature);
        }
        self
    }

    pub const fn top_p(mut self, top_p: Option<f32>) -> Self {
        if let Some(top_p) = top_p {
            self.top_p = Some(top_p);
        }
        self
    }

    pub const fn max_tokens(mut self, max_tokens: Option<u32>) -> Self {
        if let Some(max_tokens) = max_tokens {
            self.max_tokens = Some(max_tokens);
        }
        self
    }

    #[allow(dead_code)]
    pub fn stop(mut self, stop: Vec<String>) -> Self {
        self.stop = Some(stop);
        self
    }

    #[allow(dead_code)]
    pub const fn presence_penalty(mut self, presence_penalty: f32) -> Self {
        self.presence_penalty = Some(presence_penalty);
        self
    }

    #[allow(dead_code)]
    pub const fn frequency_penalty(mut self, frequency_penalty: f32) -> Self {
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
    pub const fn top_k(mut self, top_k: u32) -> Self {
        self.top_k = Some(top_k);
        self
    }

    #[allow(dead_code)]
    pub const fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub async fn send_message(
        &mut self,
        message: Message,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        self.messages.push(message);

        let prompt = match &self.provider {
            Provider::Anthropic => serde_json::to_value(AnthropicRequest {
                model: self.model.clone(),
                temperature: self.temperature,
                top_p: self.top_p,
                max_tokens: self.max_tokens.unwrap_or(4096),
                system: self.system.clone(),
                messages: self.messages.clone(),
                top_k: self.top_k,
                stream: self.stream,
            })?,
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
            Provider::Google => serde_json::to_value(GoogleRequest {
                system_instruction: SystemInstruction {
                    parts: Part {
                        text: self.system.clone(),
                    },
                },
                contents: self.messages.iter().map(Instruction::from).collect(),
            })?,
            Provider::Mistral => json!({}),
            Provider::Ollama => json!({
                "model": self.model,
                "temperature": self.temperature,
                "top_p": self.top_p,
                "max_tokens": self.max_tokens,
                "stream": self.stream,
                "messages": self.messages,
                "presence_penalty": self.presence_penalty,
                "frequency_penalty": self.frequency_penalty,
                "stop": self.stop,
            }),
        };

        let request_url = self.get_request_url();

        let req_base = Client::new()
            .post(request_url)
            .json(&prompt)
            .header("content-type", "application/json");

        let req = match &self.provider {
            Provider::Anthropic => req_base
                .header("anthropic-version", "2023-06-01")
                .header("x-api-key", self.token.to_string()),
            Provider::OpenAI | Provider::Mistral => req_base.bearer_auth(self.token.to_string()),
            Provider::Google | Provider::Ollama => req_base,
        };

        let response = req.send().await?;

        if response.status().is_success() {
            let message = match &self.provider {
                Provider::Anthropic => {
                    let anth_response = response.json::<AnthropicResponse>().await?;
                    anth_response.into_message()
                }
                Provider::OpenAI | Provider::Ollama => {
                    let ai_response = response.json::<OpenAIResponse>().await?;
                    ai_response.into_message()
                }
                Provider::Mistral => {
                    let mistral_response = response.json::<MistralResponse>().await?;
                    mistral_response.into_message()
                }
                Provider::Google => {
                    let google_response = response.json::<GoogleResponse>().await?;
                    google_response.into_message()
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
            Provider::Anthropic | Provider::Google => {
                let mut result = vec![Message {
                    role: Role::System,
                    content: self.system.to_string(),
                }];
                result.append(&mut msgs);
                result
            }
            Provider::OpenAI | Provider::Mistral | Provider::Ollama => msgs,
        }
    }

    fn get_request_url(&self) -> String {
        match &self.provider {
            Provider::Anthropic => "https://api.anthropic.com/v1/messages".to_string(),
            Provider::OpenAI => "https://api.openai.com/v1/chat/completions".to_string(),
            Provider::Mistral => "https://api.mistral.ai/v1/chat/completions".to_string(),
            Provider::Google => format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
                self.model, self.token
            ),
            Provider::Ollama => "http://localhost:11434".to_string(),
        }
    }
}
