use std::env;

use crate::llm_api::{log_debug_response, send_debug_request};

use super::{Backend, BackendError, ChatCompletionRequest, JsonSchema, ToolDefinition};
use async_trait::async_trait;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug)]
pub struct Anthropic {
    model: String,
}

impl Anthropic {
    pub const fn new(model: String) -> Self {
        Self { model }
    }

    fn get_model_name(&self) -> String {
        match self.model.to_lowercase().as_str() {
            "opus" => "claude-3-opus-20240229".into(),
            "sonnet" => "claude-3-5-sonnet-20240620".into(),
            "sonnet3" => "claude-3-sonnet-20240229".into(),
            "haiku" => "claude-3-haiku-20240307".into(),
            _ => self.model.clone(),
        }
    }

    fn build_request_body(
        &self,
        request: &ChatCompletionRequest,
        tools: &[&dyn ToolDefinition],
    ) -> Value {
        let model = self.get_model_name();
        let max_tokens = if model == "claude-3-5-sonnet-20240620" {
            8192
        } else {
            4096
        };

        serde_json::to_value(Request {
            model,
            temperature: request.temperature,
            top_p: request.top_p,
            max_tokens: request.max_tokens.unwrap_or(max_tokens),
            system: request.system_prompt.clone(),
            messages: request
                .messages
                .clone()
                .iter()
                .filter_map(|m| m.try_into().ok())
                .collect::<Vec<Message>>(),
            top_k: request.top_k,
            stream: request.stream,
            stop_sequences: None,
            tool_choice: None,
            tools: &tools.iter().map(|&t| t.into()).collect::<Box<[Tool]>>(),
        })
        .unwrap_or_default()
    }

    async fn send_api_request(
        &self,
        request_body: Value,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let api_token = env::var("CLAUDE_API_KEY").ok();
        let request_url = "https://api.anthropic.com/v1/messages".to_string();

        let client = reqwest::Client::new();
        let req_base = client
            .post(request_url)
            .json(&request_body)
            .header("content-type", "application/json")
            .header("anthropic-version", "2023-06-01")
            .header("x-api-key", api_token.unwrap());

        let req = if self.get_model_name() == "claude-3-5-sonnet-20240620" {
            req_base.header("anthropic-beta", "max-tokens-3-5-sonnet-2024-07-15")
        } else {
            req_base
        };

        req.send().await
    }

    fn parse_response(&self, anth_response: &Response) -> Option<super::open_ai::Message> {
        debug!(target: "acai", "Anthropic response: {:#?}", anth_response);
        let anth_message = Message::try_from(anth_response);
        debug!(target: "acai", "Anthropic message: {:#?}", anth_message);
        match anth_message {
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
    }
}

#[async_trait]
impl Backend for Anthropic {
    async fn chat(
        &self,
        request: ChatCompletionRequest,
        tools: &[&dyn ToolDefinition],
    ) -> Result<super::open_ai::Message, BackendError> {
        let request_body = self.build_request_body(&request, tools);

        debug!(target: "acai", "{request_body:#?}");

        let response = self.send_api_request(request_body.clone()).await?;

        debug!(target: "acai", "Response status: {:?}", response.status());

        if response.status().is_success() {
            let anth_response = match response.json::<Response>().await {
                Ok(r) => Some(r),
                Err(e) => {
                    error!("Error parsing response: {e:#?}");
                    None
                }
            };
            let message = anth_response.and_then(|ar| self.parse_response(&ar));

            debug!(target: "acai", "{message:#?}");

            if message.is_none() {
                let test_req = self.send_api_request(request_body.clone()).await?;
                let _ = log_debug_response(test_req).await;
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
            let test_req = self.send_api_request(request_body.clone()).await?;
            let _ = log_debug_response(test_req).await;
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

#[derive(Serialize)]
pub struct ToolChoice {
    #[serde(rename = "type")]
    r#type: String,
    name: Option<String>,
}

#[derive(Serialize)]
pub struct Tool {
    name: String,
    description: Option<String>,
    input_schema: JsonSchema,
}

impl From<&dyn ToolDefinition> for Tool {
    fn from(tool: &dyn ToolDefinition) -> Self {
        Self {
            description: Some(tool.description().to_owned()),
            name: tool.name().to_owned(),
            input_schema: tool.get_parameters(),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum UserContent {
    Text {
        text: String,
    },
    ToolResult {
        tool_use_id: String,
        is_error: bool,
        content: String,
    },
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum AssistantContent {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Map<String, Value>,
    },
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "role")]
pub enum Message {
    User { content: Vec<UserContent> },
    Assistant { content: Vec<AssistantContent> },
}

impl TryFrom<&super::open_ai::Message> for Message {
    type Error = String;

    fn try_from(msg: &super::open_ai::Message) -> Result<Self, Self::Error> {
        match msg {
            super::open_ai::Message::System {
                content: _,
                name: _,
            } => todo!(),
            super::open_ai::Message::User { content, name: _ } => Ok(Self::User {
                content: vec![UserContent::Text {
                    text: content.to_string(),
                }],
            }),
            super::open_ai::Message::Assistant {
                content,
                name: _,
                tool_calls,
            } => tool_calls.as_ref().map_or_else(
                || {
                    content.clone().map_or_else(
                        || Err("content was empty".to_string()),
                        |content| {
                            Ok(Self::Assistant {
                                content: vec![AssistantContent::Text { text: content }],
                            })
                        },
                    )
                },
                |tool| {
                    tool.first().map_or_else(
                        || Err("missing tool".to_owned()),
                        |tc| {
                            Ok(Self::Assistant {
                                content: vec![AssistantContent::ToolUse {
                                    id: tc.id.to_string(),
                                    name: tc.clone().function.name,
                                    input: serde_json::from_str::<Map<String, Value>>(
                                        &tc.clone().function.arguments,
                                    )
                                    .unwrap(),
                                }],
                            })
                        },
                    )
                },
            ),
            super::open_ai::Message::Tool {
                content,
                tool_call_id,
            } => Ok(Self::User {
                content: vec![UserContent::ToolResult {
                    tool_use_id: tool_call_id.to_string(),
                    is_error: false,
                    content: content.to_string(),
                }],
            }),
        }
    }
}

impl TryFrom<&Response> for Message {
    type Error = String;

    fn try_from(value: &Response) -> Result<Self, Self::Error> {
        let stop_reason = value.stop_reason.clone();
        let item = if matches!(stop_reason, StopReason::ToolUse) {
            value.content.get(1)
        } else {
            value.content.first()
        };
        if let Some(content) = item {
            let a = match content {
                ResponseContent::Text { text } => Self::Assistant {
                    content: vec![AssistantContent::Text {
                        text: text.to_string(),
                    }],
                },
                ResponseContent::ToolUse { id, name, input } => Self::Assistant {
                    content: vec![AssistantContent::ToolUse {
                        id: id.to_string(),
                        name: name.to_string(),
                        input: input.clone(),
                    }],
                },
            };
            return Ok(a);
        }
        Err("No message in response.".to_string())
    }
}

#[derive(Serialize)]
pub struct Request<'a> {
    pub model: String,
    pub max_tokens: u32,
    pub system: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: &'a [Tool],
    pub stream: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Response {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub role: String,
    pub content: Vec<ResponseContent>,
    pub model: String,
    pub stop_reason: StopReason,
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum ResponseContent {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Map<String, Value>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Usage {
    input_tokens: u32,
    output_tokens: u32,
}
