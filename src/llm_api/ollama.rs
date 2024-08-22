use crate::llm_api::send_debug_request;

use super::{Backend, BackendError, ChatCompletionRequest, ToolDefinition};
use async_trait::async_trait;
use log::debug;
use serde_json::{json, Value};

#[derive(Debug)]
pub struct Ollama {
    model: String,
}

impl Ollama {
    pub const fn new(model: String) -> Self {
        Self { model }
    }
}

#[async_trait]
impl Backend for Ollama {
    async fn chat(
        &self,
        request: ChatCompletionRequest,
        tools: &[&dyn ToolDefinition],
    ) -> Result<super::open_ai::Message, BackendError> {
        let request_url = "http://localhost:11434".to_string();

        let request_body = json!({
            "model": self.model,
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
                .collect::<Box<[super::open_ai::Tool]>>(),
        });

        debug!(target: "acai", "{request_url}");
        debug!(target: "acai", "{request_body}");

        let client = reqwest::Client::new();
        let req = client
            .post(request_url)
            .json(&request_body)
            .header("content-type", "application/json");

        let test_req = req.try_clone().unwrap();

        let response = req.send().await?;

        debug!(target: "acai", "Response status: {:?}", response.status());

        if response.status().is_success() {
            let ai_response = response.json::<super::open_ai::Response>().await?;
            let openai_message = super::open_ai::Message::try_from(&ai_response);
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
