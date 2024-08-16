use std::env;

use crate::llm_api::send_debug_request;

use super::{Backend, BackendError, ChatCompletionRequest, ToolDefinition};
use async_trait::async_trait;
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug)]
pub struct Mistral {
    model: String,
}

impl Mistral {
    pub const fn new(model: String) -> Self {
        Self { model }
    }
}

#[async_trait]
impl Backend for Mistral {
    async fn chat(
        &self,
        request: ChatCompletionRequest,
        _tools: &[&dyn ToolDefinition],
    ) -> Result<super::open_ai::Message, BackendError> {
        let model = match self.model.to_lowercase().as_str() {
            "codestral" => "codestral-latest".into(),
            _ => self.model.clone(),
        };
        let api_token = env::var("MISTRAL_API_KEY").ok();
        let request_url = "https://api.mistral.ai/v1/chat/completions".to_string();

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
            let mistral_response = response.json::<Response>().await?;
            let mistral_message = Message::try_from(&mistral_response);
            let message =
                mistral_message.map_or(None, |msg| super::open_ai::Message::try_from(&msg).ok());

            debug!(target: "acai", "{message:?}");

            if message.is_none() {
                let _ = send_debug_request(test_req).await;
            }

            Ok(message.expect("bad message"))
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
pub struct Response {
    pub choices: Vec<Choice>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Choice {
    pub message: Message,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "role")]
pub enum Message {
    System { content: String },
    User { content: String },
    Assistant { content: String },
    Tool { content: String },
}

impl TryFrom<&Response> for Message {
    type Error = String;

    fn try_from(value: &Response) -> Result<Self, Self::Error> {
        if let Some(choice) = value.choices.first() {
            return Ok(choice.message.clone());
        }
        Err("no message found".to_owned())
    }
}
