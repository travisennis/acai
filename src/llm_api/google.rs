use std::env;

use crate::llm_api::send_debug_request;

use super::{open_ai::Message, Backend, BackendError, ChatCompletionRequest, ToolDefinition};
use async_trait::async_trait;
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug)]
pub struct Google {
    model: String,
}

impl Google {
    pub const fn new(model: String) -> Self {
        Self { model }
    }
}

#[async_trait]
impl Backend for Google {
    async fn chat(
        &self,
        request: ChatCompletionRequest,
        _tools: &[&dyn ToolDefinition],
    ) -> Result<Message, BackendError> {
        let model = match self.model.to_lowercase().as_str() {
            "gemini-flash" => "gemini-1.5-flash-latest".into(),
            "gemini-pro" => "gemini-1.5-pro-latest".into(),
            _ => self.model.clone(),
        };
        let api_token = env::var("GOOGLE_API_KEY").ok();
        let request_url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            model,
            api_token.unwrap()
        );

        let request_body = serde_json::to_value(Request {
            system_instruction: SystemInstruction {
                parts: Part {
                    text: request.system_prompt.clone(),
                },
            },
            contents: request.messages.iter().map(Instruction::from).collect(),
        })
        .unwrap_or_default();

        debug!(target: "acai", "{request_url}");
        debug!(target: "acai", "{request_body:#?}");

        let client = reqwest::Client::new();
        let req = client
            .post(request_url)
            .json(&request_body)
            .header("content-type", "application/json");

        let test_req = req.try_clone().unwrap();

        let response = req.send().await?;

        debug!(target: "acai", "Response status: {:?}", response.status());

        if response.status().is_success() {
            let google_response = response.json::<Response>().await?;
            let google_instruction = Instruction::try_from(&google_response);
            let message =
                google_instruction.map_or(None, |msg| super::open_ai::Message::try_from(&msg).ok());

            debug!(target: "acai", "{message:?}");

            if message.is_none() {
                let _ = send_debug_request(test_req).await;
            }

            message.map_or_else(
                || Err(BackendError::RequestError("No message. Try again.".into())),
                Ok,
            )
        } else if response.status().is_server_error() {
            Err(BackendError::RequestError(
                "Service unavailable. Try again.".into(),
            ))
        } else {
            let _ = send_debug_request(test_req).await;
            match response.json::<Value>().await {
                Ok(resp_json) => match serde_json::to_string_pretty(&resp_json) {
                    Ok(resp_formatted) => Err(BackendError::RequestError(resp_formatted)),
                    Err(e) => Err(BackendError::RequestError(format!(
                        "Failed to format response JSON: {e}"
                    ))),
                },
                Err(e) => Err(BackendError::RequestError(format!(
                    "Failed to parse response JSON: {e}"
                ))),
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Part {
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SystemInstruction {
    pub parts: Part,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "role")]
pub enum Instruction {
    System { parts: Vec<Part> },
    Assistant { parts: Vec<Part> },
    User { parts: Vec<Part> },
}

impl From<&Message> for Instruction {
    fn from(value: &Message) -> Self {
        match value {
            Message::System { content, name: _ } => Self::System {
                parts: vec![Part {
                    text: content.to_string(),
                }],
            },
            Message::User { content, name: _ } => Self::User {
                parts: vec![Part {
                    text: content.to_string(),
                }],
            },
            Message::Assistant {
                content,
                name: _,
                tool_calls: _,
            } => Self::Assistant {
                parts: vec![Part {
                    text: content.clone().unwrap_or_default(),
                }],
            },
            Message::Tool {
                content: _,
                tool_call_id: _,
            } => todo!(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Request {
    #[serde(rename = "systemInstruction")]
    pub system_instruction: SystemInstruction,
    pub contents: Vec<Instruction>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Content {
    pub parts: Vec<Part>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Candidate {
    pub content: Content,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub candidates: Vec<Candidate>,
}

impl TryFrom<&Response> for Instruction {
    type Error = String;

    fn try_from(value: &Response) -> Result<Self, Self::Error> {
        if let Some(candidate) = value.candidates.first() {
            if let Some(part) = candidate.content.parts.first() {
                return Ok(Self::Assistant {
                    parts: vec![Part {
                        text: part.text.clone(),
                    }],
                });
            }
        }
        Err("No message in response.".to_string())
    }
}
