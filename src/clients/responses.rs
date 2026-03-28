use log::debug;

use crate::config::model::ResolvedModelConfig;
use crate::models::Role;

use crate::clients::agent::TurnResult;
use crate::clients::tools::Tool;
use crate::clients::types::{
    ApiResponse, ApiUsage, ConversationItem, InputTokensDetails, OutputTokensDetails,
    ProviderConfig, ReasoningConfig, Request, Usage,
};

// =============================================================================
// Responses API Backend
// =============================================================================

/// Send a request to the Responses API, returning the raw HTTP response.
///
/// # Errors
///
/// Returns an error if the HTTP request fails.
pub(super) async fn send_request(
    client: &reqwest::Client,
    config: &ResolvedModelConfig,
    history: &[ConversationItem],
    tools: &[Tool],
) -> anyhow::Result<reqwest::Response> {
    let provider_config = if config.config.providers.is_empty()
        || (config.config.providers.len() == 1 && config.config.providers[0] == "all")
    {
        None
    } else {
        Some(ProviderConfig {
            only: config.config.providers.clone(),
        })
    };

    let reasoning = (config.config.reasoning_effort.is_some()
        || config.config.reasoning_summary.is_some()
        || config.config.reasoning_max_tokens.is_some())
    .then(|| ReasoningConfig {
        effort: config.config.reasoning_effort.clone(),
        summary: config.config.reasoning_summary.clone(),
        max_tokens: config.config.reasoning_max_tokens,
    });

    let prompt = Request {
        model: &config.config.model,
        input: build_input(history),
        temperature: config.config.temperature,
        top_p: config.config.top_p,
        max_output_tokens: config.config.max_output_tokens,
        tools: Some(tools.to_vec()),
        tool_choice: Some("auto".to_string()),
        provider: provider_config,
        reasoning,
    };

    let url = format!("{}/responses", config.config.base_url.trim_end_matches('/'));
    debug!(target: "acai", "{url}");
    let prompt_json = serde_json::to_string(&prompt)?;
    debug!(target: "acai", "{prompt_json}");

    let response = client
        .post(&url)
        .json(&prompt)
        .header("content-type", "application/json")
        .header("HTTP-Referer", "https://github.com/travisennis/acai")
        .header("X-Title", "acai")
        .bearer_auth(&config.api_key)
        .send()
        .await?;

    Ok(response)
}

/// Parse an HTTP response from the Responses API into a `TurnResult`.
///
/// # Errors
///
/// Returns an error if the response body cannot be deserialized.
pub(super) async fn parse_response(response: reqwest::Response) -> anyhow::Result<TurnResult> {
    let api_response = response.json::<ApiResponse>().await?;
    debug!(target: "acai", "{api_response:?}");

    let usage = api_response.usage.as_ref().map(map_usage);
    let items = parse_output_items(&api_response);

    Ok(TurnResult { items, usage })
}

/// Map API-level usage to the canonical `Usage` type.
fn map_usage(api_usage: &ApiUsage) -> Usage {
    Usage {
        input_tokens: api_usage.input_tokens.unwrap_or(0),
        output_tokens: api_usage.output_tokens.unwrap_or(0),
        total_tokens: api_usage.total_tokens.unwrap_or(0),
        input_tokens_details: InputTokensDetails {
            cached_tokens: api_usage
                .input_tokens_details
                .as_ref()
                .map_or(0, |d| d.cached_tokens.unwrap_or(0)),
        },
        output_tokens_details: OutputTokensDetails {
            reasoning_tokens: api_usage
                .output_tokens_details
                .as_ref()
                .map_or(0, |d| d.reasoning_tokens.unwrap_or(0)),
        },
    }
}

/// Build the input array for the Responses API from conversation history.
fn build_input(history: &[ConversationItem]) -> Vec<serde_json::Value> {
    history.iter().map(ConversationItem::to_api_input).collect()
}

/// Parse the output items from an API response into `ConversationItem` values.
fn parse_output_items(api_response: &ApiResponse) -> Vec<ConversationItem> {
    let mut items = Vec::new();

    for output in &api_response.output {
        match output.msg_type.as_str() {
            "reasoning" => {
                if let Some(id) = &output.id {
                    let summary = output
                        .summary
                        .clone()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| {
                            output
                                .content
                                .as_ref()
                                .map(|c| {
                                    c.iter()
                                        .filter(|item| item.content_type == "reasoning_text")
                                        .filter_map(|item| item.text.clone())
                                        .collect()
                                })
                                .unwrap_or_default()
                        });

                    let content = output.content.as_ref().map(|c| {
                        c.iter()
                            .map(|item| super::types::ReasoningContent {
                                content_type: item.content_type.clone(),
                                text: item.text.clone(),
                            })
                            .collect()
                    });

                    items.push(ConversationItem::Reasoning {
                        id: id.clone(),
                        summary,
                        encrypted_content: output.encrypted_content.clone(),
                        content,
                    });
                }
            },
            "function_call" => {
                items.push(ConversationItem::FunctionCall {
                    id: output.id.clone().unwrap_or_default(),
                    call_id: output.call_id.clone().unwrap_or_default(),
                    name: output.name.clone().unwrap_or_default(),
                    arguments: output.arguments.clone().unwrap_or_default(),
                });
            },
            "message" => {
                let text = output
                    .content
                    .as_ref()
                    .and_then(|c| c.iter().find(|item| item.content_type == "output_text"))
                    .and_then(|item| item.text.clone())
                    .unwrap_or_default();

                items.push(ConversationItem::Message {
                    role: Role::Assistant,
                    content: text,
                    id: output.id.clone(),
                    status: output.status.clone(),
                });
            },
            _ => {},
        }
    }

    items
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::clients::types::{OutputContent, OutputMessage};

    #[test]
    fn build_input_converts_history() {
        let history = vec![ConversationItem::Message {
            role: Role::User,
            content: "hi".to_string(),
            id: None,
            status: None,
        }];
        let input = build_input(&history);
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["type"], "message");
    }

    #[test]
    fn build_input_empty_history() {
        let history: Vec<ConversationItem> = vec![];
        let input = build_input(&history);
        assert!(input.is_empty());
    }

    #[test]
    fn parse_output_items_message() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "message".to_string(),
                id: Some("msg-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: Some("assistant".to_string()),
                status: Some("completed".to_string()),
                content: Some(vec![OutputContent {
                    content_type: "output_text".to_string(),
                    text: Some("Hello!".to_string()),
                }]),
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::Message {
            role: Role::Assistant, content, ..
        } if content == "Hello!"));
    }

    #[test]
    fn parse_output_items_function_call() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "function_call".to_string(),
                id: Some("fc-1".to_string()),
                call_id: Some("call-1".to_string()),
                name: Some("bash".to_string()),
                arguments: Some(r#"{"cmd":"ls"}"#.to_string()),
                role: None,
                status: None,
                content: None,
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::FunctionCall {
            name, ..
        } if name == "bash"));
    }

    #[test]
    fn parse_output_items_reasoning() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "reasoning".to_string(),
                id: Some("r-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: Some(vec![OutputContent {
                    content_type: "reasoning_text".to_string(),
                    text: Some("thinking...".to_string()),
                }]),
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::Reasoning { .. }));
    }

    #[test]
    fn parse_output_items_reasoning_with_encrypted_content() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "reasoning".to_string(),
                id: Some("r-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: None,
                encrypted_content: Some("gAAAAABencrypted...".to_string()),
                summary: Some(vec!["step 1".to_string(), "step 2".to_string()]),
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        if let ConversationItem::Reasoning {
            summary,
            encrypted_content,
            ..
        } = &items[0]
        {
            assert_eq!(summary.len(), 2);
            assert_eq!(summary[0], "step 1");
            assert_eq!(encrypted_content.as_deref(), Some("gAAAAABencrypted..."));
        } else {
            panic!("Expected Reasoning item");
        }
    }

    #[test]
    fn parse_output_items_reasoning_preserves_content_for_roundtrip() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "reasoning".to_string(),
                id: Some("r-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: Some(vec![OutputContent {
                    content_type: "reasoning_text".to_string(),
                    text: Some("deep reasoning here".to_string()),
                }]),
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        let api_input = items[0].to_api_input();
        assert_eq!(api_input["content"][0]["type"], "reasoning_text");
        assert_eq!(api_input["content"][0]["text"], "deep reasoning here");
    }

    #[test]
    fn parse_output_items_unknown_type_ignored() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "unknown_type".to_string(),
                id: None,
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: None,
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert!(items.is_empty());
    }

    #[test]
    fn parse_output_items_multiple_items() {
        let response = ApiResponse {
            id: None,
            output: vec![
                OutputMessage {
                    msg_type: "reasoning".to_string(),
                    id: Some("r-1".to_string()),
                    call_id: None,
                    name: None,
                    arguments: None,
                    role: None,
                    status: None,
                    content: Some(vec![OutputContent {
                        content_type: "reasoning_text".to_string(),
                        text: Some("thinking...".to_string()),
                    }]),
                    encrypted_content: None,
                    summary: None,
                },
                OutputMessage {
                    msg_type: "message".to_string(),
                    id: Some("msg-1".to_string()),
                    call_id: None,
                    name: None,
                    arguments: None,
                    role: None,
                    status: None,
                    content: Some(vec![OutputContent {
                        content_type: "output_text".to_string(),
                        text: Some("Hello!".to_string()),
                    }]),
                    encrypted_content: None,
                    summary: None,
                },
            ],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn parse_output_items_message_without_content() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "message".to_string(),
                id: Some("msg-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: None,
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0], ConversationItem::Message {
            content, ..
        } if content.is_empty()));
    }

    #[test]
    fn provider_config_with_all_returns_none() {
        let providers = vec!["all".to_string()];
        let config = if providers.is_empty() || (providers.len() == 1 && providers[0] == "all") {
            None
        } else {
            Some(ProviderConfig { only: providers })
        };
        assert!(config.is_none());
    }

    // =========================================================================
    // Malformed Response Tests
    // =========================================================================

    #[test]
    fn parse_output_items_empty_output_array() {
        let response = ApiResponse {
            id: None,
            output: vec![],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert!(items.is_empty());
    }

    #[test]
    fn parse_output_items_missing_id_for_reasoning() {
        // Reasoning without an id should be skipped (id is required for reasoning)
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "reasoning".to_string(),
                id: None, // Missing required id
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: Some(vec![OutputContent {
                    content_type: "reasoning_text".to_string(),
                    text: Some("thinking...".to_string()),
                }]),
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        // Reasoning without id is skipped
        assert!(items.is_empty());
    }

    #[test]
    fn parse_output_items_function_call_missing_fields() {
        // Function call with missing optional fields should still work
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "function_call".to_string(),
                id: None,
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: None,
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        // Should use default values
        assert!(matches!(&items[0], ConversationItem::FunctionCall {
            id,
            call_id,
            name,
            arguments,
            ..
        } if id.is_empty() && call_id.is_empty() && name.is_empty() && arguments.is_empty()));
    }

    #[test]
    fn parse_output_items_message_with_empty_content_array() {
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "message".to_string(),
                id: Some("msg-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: Some("completed".to_string()),
                content: Some(vec![]), // Empty content array
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        // Should default to empty string
        assert!(matches!(&items[0], ConversationItem::Message {
            content,
            ..
        } if content.is_empty()));
    }

    #[test]
    fn parse_output_items_message_with_non_text_content() {
        // Message with content type that isn't output_text
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "message".to_string(),
                id: Some("msg-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: Some("completed".to_string()),
                content: Some(vec![OutputContent {
                    content_type: "image".to_string(), // Not output_text
                    text: Some("image data".to_string()),
                }]),
                encrypted_content: None,
                summary: None,
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        // Should default to empty string since no output_text found
        assert!(matches!(&items[0], ConversationItem::Message {
            content,
            ..
        } if content.is_empty()));
    }

    #[test]
    fn parse_output_items_reasoning_with_summary_fallback() {
        // Reasoning with summary but no content
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "reasoning".to_string(),
                id: Some("r-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: None,
                encrypted_content: None,
                summary: Some(vec!["step 1".to_string(), "step 2".to_string()]),
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        if let ConversationItem::Reasoning { summary, .. } = &items[0] {
            assert_eq!(summary.len(), 2);
            assert_eq!(summary[0], "step 1");
        } else {
            panic!("Expected Reasoning item");
        }
    }

    #[test]
    fn parse_output_items_reasoning_content_fallback_to_summary() {
        // Reasoning with content containing reasoning_text
        let response = ApiResponse {
            id: None,
            output: vec![OutputMessage {
                msg_type: "reasoning".to_string(),
                id: Some("r-1".to_string()),
                call_id: None,
                name: None,
                arguments: None,
                role: None,
                status: None,
                content: Some(vec![OutputContent {
                    content_type: "reasoning_text".to_string(),
                    text: Some("thinking...".to_string()),
                }]),
                encrypted_content: None,
                summary: None, // No summary, should derive from content
            }],
            usage: None,
            status: None,
            error: None,
        };
        let items = parse_output_items(&response);
        assert_eq!(items.len(), 1);
        if let ConversationItem::Reasoning { summary, .. } = &items[0] {
            // Summary should be derived from content
            assert_eq!(summary.len(), 1);
            assert_eq!(summary[0], "thinking...");
        } else {
            panic!("Expected Reasoning item");
        }
    }
}

/// Tests for parsing raw HTTP responses
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod response_parsing_tests {
    use super::*;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Create a minimal valid response JSON
    fn minimal_valid_response() -> serde_json::Value {
        serde_json::json!({
            "output": [{
                "type": "message",
                "id": "msg-1",
                "status": "completed",
                "content": [{
                    "type": "output_text",
                    "text": "Hello!"
                }]
            }]
        })
    }

    #[tokio::test]
    async fn parse_response_valid_json() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(minimal_valid_response()))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/responses", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        assert_eq!(turn_result.items.len(), 1);
    }

    #[tokio::test]
    async fn parse_response_invalid_json() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not valid json{broken"))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/responses", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn parse_response_empty_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(""))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/responses", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn parse_response_missing_output_field_fails() {
        let mock_server = MockServer::start().await;

        // Response without "output" field - should fail because output is required
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "resp-123",
                "status": "completed"
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/responses", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        // Should fail because "output" is a required field
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn parse_response_with_usage() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "output": [{
                    "type": "message",
                    "id": "msg-1",
                    "status": "completed",
                    "content": [{
                        "type": "output_text",
                        "text": "Hello!"
                    }]
                }],
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 50,
                    "total_tokens": 150,
                    "input_tokens_details": {
                        "cached_tokens": 20
                    },
                    "output_tokens_details": {
                        "reasoning_tokens": 10
                    }
                }
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/responses", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        assert!(turn_result.usage.is_some());
        let usage = turn_result.usage.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
        assert_eq!(usage.input_tokens_details.cached_tokens, 20);
        assert_eq!(usage.output_tokens_details.reasoning_tokens, 10);
    }

    #[tokio::test]
    async fn parse_response_partial_usage() {
        let mock_server = MockServer::start().await;

        // Response with partial usage (some fields missing)
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "output": [{
                    "type": "message",
                    "id": "msg-1",
                    "content": [{
                        "type": "output_text",
                        "text": "Hello!"
                    }]
                }],
                "usage": {
                    "input_tokens": 100,
                    "output_tokens": 50
                    // total_tokens and details are missing
                }
            })))
            .mount(&mock_server)
            .await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/responses", mock_server.uri()))
            .send()
            .await
            .unwrap();

        let result = parse_response(response).await;
        assert!(result.is_ok());
        let turn_result = result.unwrap();
        let usage = turn_result.usage.unwrap();
        // Should use defaults for missing fields
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert_eq!(usage.total_tokens, 0); // Default
        assert_eq!(usage.input_tokens_details.cached_tokens, 0); // Default
        assert_eq!(usage.output_tokens_details.reasoning_tokens, 0); // Default
    }
}
