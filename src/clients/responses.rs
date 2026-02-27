use std::{env, error::Error};

use log::debug;
use serde::{Deserialize, Serialize};

use crate::models::{Message, Role};

const BASE_URL: &str = "https://openrouter.ai/api/v1/responses";

pub struct Responses {
    model: String,
    token: String,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_output_tokens: Option<u32>,
    system: String,
    messages: Vec<Message>,
    stream: bool,
}

impl Responses {
    pub fn new(model: String, system_prompt: &str) -> Self {
        let token = env::var("OPENROUTER_API_KEY")
            .unwrap_or_else(|_error| panic!("Error: OPENROUTER_API_KEY not set."));

        Self {
            model,
            token,
            temperature: Some(0.0),
            top_p: None,
            max_output_tokens: None,
            system: system_prompt.to_string(),
            messages: vec![Message {
                role: Role::System,
                content: system_prompt.to_string(),
            }],
            stream: false,
        }
    }

    pub fn temperature(mut self, temperature: Option<f32>) -> Self {
        if let Some(temperature) = temperature {
            self.temperature = Some(temperature);
        }
        self
    }

    pub fn top_p(mut self, top_p: Option<f32>) -> Self {
        if let Some(top_p) = top_p {
            self.top_p = Some(top_p);
        }
        self
    }

    pub fn max_output_tokens(mut self, max_output_tokens: Option<u32>) -> Self {
        if let Some(max_output_tokens) = max_output_tokens {
            self.max_output_tokens = Some(max_output_tokens);
        }
        self
    }

    pub fn stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub async fn send(
        &mut self,
        message: Message,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        self.messages.push(message);

        let prompt = Request {
            model: self.model.clone(),
            input: build_input(&self.messages),
            temperature: self.temperature,
            top_p: self.top_p,
            max_output_tokens: self.max_output_tokens,
        };

        debug!(target: "acai", "{}", BASE_URL);
        debug!(target: "acai", "{}", serde_json::to_string(&prompt)?);

        let client = reqwest::Client::new();
        let response = client
            .post(BASE_URL)
            .json(&prompt)
            .header("content-type", "application/json")
            .bearer_auth(self.token.clone())
            .send()
            .await?;

        if response.status().is_success() {
            let api_response = response.json::<ApiResponse>().await?;
            debug!(target: "acai", "{:?}", api_response);

            let message = parse_response(api_response);

            if let Some(ref msg) = message {
                self.messages.push(msg.clone());
            }

            Ok(message)
        } else {
            let error_text = response.text().await?;
            debug!(target: "acai", "{error_text}");

            match serde_json::from_str::<serde_json::Value>(&error_text) {
                Ok(resp_json) => match serde_json::to_string_pretty(&resp_json) {
                    Ok(resp_formatted) => {
                        Err(format!("{}\n\n{}", self.model, resp_formatted).into())
                    }
                    Err(e) => Err(format!("Failed to format response JSON: {e}").into()),
                },
                Err(_) => Err(format!("{}\n\n{}", self.model, error_text).into()),
            }
        }
    }

    pub fn get_message_history(&self) -> Vec<Message> {
        self.messages.clone()
    }
}

fn build_input(messages: &[Message]) -> Vec<InputMessage> {
    messages
        .iter()
        .map(|msg| InputMessage {
            msg_type: "message".to_string(),
            role: match msg.role {
                Role::System => "system".to_string(),
                Role::User => "user".to_string(),
                Role::Assistant => "assistant".to_string(),
            },
            content: vec![ContentBlock {
                content_type: "input_text".to_string(),
                text: msg.content.clone(),
            }],
        })
        .collect()
}

fn parse_response(api_response: ApiResponse) -> Option<Message> {
    // Find the message output (skip reasoning blocks)
    let output = api_response
        .output
        .into_iter()
        .find(|o| o.msg_type == "message")?;

    let content = output.content.into_iter().next()?;

    if content.content_type == "output_text" {
        Some(Message {
            role: Role::Assistant,
            content: content.text.unwrap_or_default(),
        })
    } else {
        None
    }
}

#[derive(Serialize)]
struct Request {
    model: String,
    input: Vec<InputMessage>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_output_tokens: Option<u32>,
}

#[derive(Serialize)]
struct InputMessage {
    #[serde(rename = "type")]
    msg_type: String,
    role: String,
    content: Vec<ContentBlock>,
}

#[derive(Serialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

#[derive(Deserialize, Debug)]
struct ApiResponse {
    id: Option<String>,
    output: Vec<OutputMessage>,
    usage: Option<Usage>,
    status: Option<String>,
    error: Option<ApiError>,
}

#[derive(Deserialize, Debug)]
struct OutputMessage {
    #[serde(rename = "type")]
    msg_type: String,
    role: Option<String>,
    status: Option<String>,
    content: Vec<OutputContent>,
}

#[derive(Deserialize, Debug)]
struct OutputContent {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Deserialize, Debug)]
struct Usage {
    #[serde(rename = "input_tokens")]
    input_tokens: Option<u32>,
    #[serde(rename = "output_tokens")]
    output_tokens: Option<u32>,
    #[serde(rename = "total_tokens")]
    total_tokens: Option<u32>,
}

#[derive(Deserialize, Debug)]
struct ApiError {
    code: Option<String>,
    message: String,
    metadata: Option<serde_json::Value>,
}
