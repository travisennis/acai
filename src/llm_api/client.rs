use std::{fmt::Display, str::FromStr};

use async_trait::async_trait;
use log::{debug, error};
use reqwest::RequestBuilder;

use super::{open_ai::Message, JsonSchema};

#[derive(Clone)]
pub enum Provider {
    /// Anthropic, provider of the Claude family of language models
    Anthropic(String),
    /// `OpenAI`, provider of GPT models including `ChatGPT`
    OpenAI(String),
    /// Mistral AI, provider of open-source language models
    Mistral(String),
    /// Google, provider of various AI models including `PaLM` and Gemini
    Google(String),
    /// Ollama, an open-source platform for running language models locally
    Ollama(String),
}

impl FromStr for Provider {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = input.split('/').collect();
        let (provider, model) = match parts.len() {
            2 => Ok((parts[0].trim().to_owned(), parts[1].trim().to_owned())),
            3 => {
                let model = parts[1].trim().to_string();
                let combined_model = model + "/" + parts[2].trim();
                Ok((parts[0].trim().to_owned(), combined_model))
            }
            _ => Err("Invalid format. Expected 'provider/model'".to_string()),
        }?;

        match provider.to_lowercase().as_str() {
            "anthropic" => Ok(Self::Anthropic(model)),
            "openai" => Ok(Self::OpenAI(model)),
            "mistral" => Ok(Self::Mistral(model)),
            "google" => Ok(Self::Google(model)),
            "ollama" => Ok(Self::Ollama(model)),
            _ => Err(format!("Unknown provider: {provider}")),
        }
    }
}

impl Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self {
            Self::Anthropic(model) => write!(f, "anthropic/{model}"),
            Self::OpenAI(model) => write!(f, "openai/{model}"),
            Self::Mistral(model) => write!(f, "mistral/{model}"),
            Self::Google(model) => write!(f, "google/{model}"),
            Self::Ollama(model) => write!(f, "ollama/{model}"),
        }
    }
}

impl Provider {
    pub fn init_messages(&self, system_prompt: &str) -> Vec<Message> {
        match self {
            Self::OpenAI(_) | Self::Mistral(_) | Self::Ollama(_) => vec![Message::System {
                content: system_prompt.to_string(),
                name: None,
            }],
            Self::Google(_) | Self::Anthropic(_) => vec![],
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum BackendError {
    #[error("Request error {0}")]
    RequestError(String),
    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),
}

#[async_trait]
pub trait Backend: std::fmt::Debug + Send + Sync {
    async fn chat(
        &self,
        request: ChatCompletionRequest,
        tools: &[&dyn ToolDefinition],
    ) -> Result<Message, BackendError>;
}

pub fn create(config: Provider) -> Box<dyn Backend> {
    match config {
        Provider::Anthropic(model) => Box::new(super::anthropic::Anthropic::new(model)),
        Provider::Mistral(model) => Box::new(super::mistral::Mistral::new(model)),
        Provider::Google(model) => Box::new(super::google::Google::new(model)),
        Provider::Ollama(model) => Box::new(super::ollama::Ollama::new(model)),
        Provider::OpenAI(model) => Box::new(super::open_ai::OpenAI::new(model)),
    }
}

pub async fn send_debug_request(
    test_req: RequestBuilder,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let debug_response = test_req.send().await?;
    debug!(target: "acai", "{}", debug_response.status());
    debug!(target: "acai", "{:?}", debug_response.text().await?);
    Ok(())
}

pub trait ToolDefinition: std::marker::Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn get_parameters(&self) -> JsonSchema;
}

#[async_trait]
pub trait CallableTool: std::fmt::Debug + Send + Sync {
    async fn call(&self, arguments: serde_json::Value) -> Result<serde_json::Value, &'static str>;
}

pub struct ChatCompletionRequest {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_tokens: Option<u32>,
    pub system_prompt: String,
    pub messages: Vec<Message>,
    pub stop: Option<Vec<String>>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub logit_bias: Option<std::collections::HashMap<String, f32>>,
    pub user: Option<String>,
    pub top_k: Option<u32>,
    pub stream: bool,
}

impl Default for ChatCompletionRequest {
    fn default() -> Self {
        Self {
            temperature: None,
            top_p: None,
            max_tokens: None,
            system_prompt: String::new(),
            messages: [].to_vec(),
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            logit_bias: None,
            user: None,
            top_k: None,
            stream: false,
        }
    }
}
