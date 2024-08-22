use std::env;

use crate::llm_api::send_debug_request;

use super::{
    open_ai::Message, Backend, BackendError, ChatCompletionRequest, JsonSchema, ToolDefinition,
};
use async_trait::async_trait;
use log::{debug, error};
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
        tools: &[&dyn ToolDefinition],
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
                parts: Part::Text(request.system_prompt.clone()),
            },
            contents: request.messages.iter().map(Instruction::from).collect(),
            tools: &tools.iter().map(|&t| t.into()).collect::<Box<[Tool]>>(),
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
            let google_response = match response.json::<Response>().await {
                Ok(r) => Some(r),
                Err(e) => {
                    error!("Error parsing response: {e:#?}");
                    None
                }
            };
            let message = google_response.and_then(|google_response| {
                debug!(target: "acai", "Google response: {:#?}", google_response);
                let google_message = Instruction::try_from(&google_response);
                debug!(target: "acai", "Google message: {:#?}", google_message);
                match google_message {
                    Ok(msg) => match super::open_ai::Message::try_from(&msg) {
                        Ok(final_msg) => Some(final_msg),
                        Err(e) => {
                            error!("Error parsing message: {e:#?}");
                            None
                        }
                    },
                    Err(e) => {
                        error!("Error parsing message: {e}");
                        None
                    }
                }
            });

            debug!(target: "acai", "{message:#?}");

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
pub struct FunctionCall {
    pub name: String,
    pub args: Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FunctionResponse {
    pub name: String,
    pub response: Value,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Part {
    Text(String),
    FunctionCall(FunctionCall),
    FunctionResponse(FunctionResponse),
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
    Model { parts: Vec<Part> },
    User { parts: Vec<Part> },
}

impl From<&Message> for Instruction {
    fn from(value: &Message) -> Self {
        match value {
            Message::System { content, name: _ } => Self::System {
                parts: vec![Part::Text(content.to_string())],
            },
            Message::User { content, name: _ } => Self::User {
                parts: vec![Part::Text(content.to_string())],
            },
            Message::Assistant {
                content,
                name: _,
                tool_calls: _,
            } => Self::Model {
                parts: vec![Part::Text(content.clone().unwrap_or_default())],
            },
            Message::Tool {
                content: _,
                tool_call_id: _,
            } => todo!(),
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Request<'a> {
    #[serde(rename = "systemInstruction")]
    pub system_instruction: SystemInstruction,
    pub contents: Vec<Instruction>,
    pub tools: &'a [Tool],
}

#[derive(Serialize, Debug)]
pub struct Tool {
    pub function_declarations: Vec<FunctionDeclaration>,
}

#[derive(Serialize, Debug)]
pub struct FunctionDeclaration {
    pub name: String,
    pub description: String,
    pub parameters: JsonSchema,
}

impl From<&dyn ToolDefinition> for Tool {
    fn from(tool: &dyn ToolDefinition) -> Self {
        Self {
            function_declarations: vec![FunctionDeclaration {
                description: tool.description().to_owned(),
                name: tool.name().to_owned(),
                parameters: tool.get_parameters(),
            }],
        }
    }
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
                return match part {
                    Part::Text(text) => Ok(Self::Model {
                        parts: vec![Part::Text(text.clone())],
                    }),
                    Part::FunctionCall(function_call) => Ok(Self::Model {
                        parts: vec![Part::FunctionCall(FunctionCall {
                            name: function_call.name.clone(),
                            args: function_call.args.clone(),
                        })],
                    }),
                    Part::FunctionResponse(function_response) => Ok(Self::Model {
                        parts: vec![Part::FunctionResponse(FunctionResponse {
                            name: function_response.name.clone(),
                            response: function_response.response.clone(),
                        })],
                    }),
                };
            }
        }
        Err("No message in response.".to_string())
    }
}
