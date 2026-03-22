use log::debug;

use crate::config::model::ResolvedModelConfig;
use crate::models::Role;

use super::agent::TurnResult;
use super::tools::Tool;
use super::types::{
    ApiResponse, ApiUsage, ConversationItem, InputTokensDetails, OutputTokensDetails,
    ProviderConfig, Request, Usage,
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

    let prompt = Request {
        model: &config.config.model,
        input: build_input(history),
        temperature: config.config.temperature,
        top_p: config.config.top_p,
        max_output_tokens: config.config.max_output_tokens,
        tools: Some(tools.to_vec()),
        tool_choice: Some("auto".to_string()),
        provider: provider_config,
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
    use super::super::types::{OutputContent, OutputMessage};
    use super::*;

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
}
