use std::env;

use crate::llm_api::send_debug_request;

use super::{Backend, BackendError, ChatCompletionRequest, JsonSchema, ToolDefinition};
use async_trait::async_trait;
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug)]
pub struct OpenAI {
    model: String,
}

impl OpenAI {
    pub const fn new(model: String) -> Self {
        Self { model }
    }
}

#[async_trait]
impl Backend for OpenAI {
    async fn chat(
        &self,
        request: ChatCompletionRequest,
        tools: &[&dyn ToolDefinition],
    ) -> Result<Message, BackendError> {
        let model = match self.model.to_lowercase().as_str() {
            "gpt-4o" | "gpt4o" => "gpt-4o".into(),
            "gpt-4o-mini" | "gpt4omini" => "gpt-4o-mini".into(),
            "gpt-4-turbo" | "gtp4turbo" => "gpt-4-turbo-preview".into(),
            "gpt-4" | "gtp4" => "gpt-4-0314".into(),
            "gpt-3.5-turbo" | "gpt35turbo" => "gpt-3.5-turbo".into(),
            _ => self.model.clone(),
        };

        let api_token = env::var("OPENAI_API_KEY").ok();
        let request_url = "https://api.openai.com/v1/chat/completions".to_string();

        let request_body = json!({
            "model": model,
            "temperature": request.temperature,
            "top_p": request.top_p,
            "max_tokens": request.max_tokens,
            "stream": request.stream,
            "messages": request.messages,
            "presence_penalty": request.presence_penalty,
            "frequency_penalty": request.frequency_penalty,
            "stop": request.stop,
            "logit_bias": request.logit_bias,
            "user": request.user,
            "tools": &tools
                .iter()
                .map(|&t| t.into())
                .collect::<Box<[Tool]>>(),
        });

        debug!(target: "acai", "{request_url}");
        debug!(target: "acai", "{request_body}");

        let client = reqwest::Client::new();
        let req = client
            .post(request_url)
            .json(&request_body)
            .bearer_auth(api_token.unwrap())
            .header("content-type", "application/json");

        let test_req = req.try_clone().unwrap();

        let response = req.send().await?;

        debug!(target: "acai", "Response status: {:?}", response.status());

        if response.status().is_success() {
            let ai_response = response.json::<Response>().await?;
            let openai_message = Message::try_from(&ai_response);
            let message = openai_message.ok();

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

#[derive(Serialize, Clone, Debug)]
pub struct Function {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub name: String,
    pub parameters: JsonSchema,
}

#[derive(Serialize, Clone, Debug)]
pub struct Tool {
    #[serde(rename = "type")]
    pub r#type: String,
    pub function: Function,
}

impl From<&dyn ToolDefinition> for Tool {
    fn from(tool: &dyn ToolDefinition) -> Self {
        Self {
            r#type: "function".to_owned(),
            function: Function {
                description: Some(tool.description().to_owned()),
                name: tool.name().to_owned(),
                parameters: tool.get_parameters(),
            },
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: FinishReason,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
}

#[derive(Serialize, Deserialize, Debug)]
#[allow(clippy::struct_field_names)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "role")]
pub enum Message {
    System {
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    User {
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Assistant {
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },
    Tool {
        content: String,
        tool_call_id: String,
    },
}

impl TryFrom<&Response> for Message {
    type Error = String;

    fn try_from(value: &Response) -> Result<Self, Self::Error> {
        if let Some(choice) = value.choices.first() {
            let msg = choice.message.clone();
            return Ok(msg);
        }
        Err("No message in response.".to_string())
    }
}

impl TryFrom<&super::anthropic::Message> for Message {
    type Error = serde_json::Error;

    fn try_from(value: &super::anthropic::Message) -> Result<Self, Self::Error> {
        match value {
            super::anthropic::Message::User { content: _ } => todo!(),
            super::anthropic::Message::Assistant { content } => match content.first().unwrap() {
                super::anthropic::AssistantContent::Text { text } => Ok(Self::Assistant {
                    content: Some(text.to_string()),
                    name: None,
                    tool_calls: None,
                }),
                super::anthropic::AssistantContent::ToolUse { id, name, input } => {
                    let arguments = serde_json::to_string(input)?;
                    Ok(Self::Assistant {
                        content: None,
                        name: None,
                        tool_calls: Some(vec![ToolCall {
                            id: id.to_string(),
                            r#type: "function".to_string(),
                            function: FunctionCall {
                                name: name.to_string(),
                                arguments,
                            },
                        }]),
                    })
                }
            },
        }
    }
}

impl TryFrom<&super::google::Instruction> for Message {
    type Error = &'static str;

    fn try_from(value: &super::google::Instruction) -> Result<Self, Self::Error> {
        match value {
            super::google::Instruction::System { parts: _ } => {
                Err("System instruction not implemented")
            }
            super::google::Instruction::Model { parts } => match parts.first().unwrap() {
                super::google::Part::Text(text) => Ok(Self::Assistant {
                    content: Some(text.clone()),
                    name: None,
                    tool_calls: None,
                }),
                super::google::Part::FunctionCall(fc) => {
                    let arguments = serde_json::to_string(&fc.args).map_err(|_| "invalid args")?;
                    Ok(Self::Assistant {
                        content: None,
                        name: None,
                        tool_calls: Some(vec![ToolCall {
                            id: String::new(),
                            r#type: "function".to_string(),
                            function: FunctionCall {
                                name: fc.name.to_string(),
                                arguments,
                            },
                        }]),
                    })
                }
                super::google::Part::FunctionResponse(_) => todo!(),
            },
            super::google::Instruction::User { parts: _ } => {
                Err("User instruction not implemented")
            }
        }
    }
}

impl TryFrom<&super::mistral::Message> for Message {
    type Error = &'static str;

    fn try_from(value: &super::mistral::Message) -> Result<Self, Self::Error> {
        match value {
            super::mistral::Message::System { content } => Ok(Self::System {
                content: content.to_string(),
                name: None,
            }),
            super::mistral::Message::User { content } => Ok(Self::User {
                content: content.to_string(),
                name: None,
            }),
            super::mistral::Message::Assistant { content } => Ok(Self::Assistant {
                content: Some(content.to_string()),
                name: None,
                tool_calls: None,
            }),
            super::mistral::Message::Tool { content: _ } => Err("Tool messages are not supported"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub function: FunctionCall,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}
