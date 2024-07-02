use crate::{clients::mistral::Response as MistralResponse, models::IntoMessage};
use core::panic;
use std::{env, error::Error};

use reqwest::Client;
use serde_json::{json, Value};

use crate::models::{Message, Role};

use super::providers::Provider;

#[allow(clippy::module_name_repetitions)]
pub struct CompletionClient {
    provider: Provider,
    model: String,
    token: String,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<u32>,
    prompt: String,
    suffix: String,
    messages: Vec<Message>,
}

impl CompletionClient {
    pub fn new(provider: Provider, model: String) -> Self {
        let token = match provider {
            Provider::Mistral => env::var("MISTRAL_API_KEY"),
            _ => todo!(),
        }
        .unwrap_or_else(|_error| panic!("Error: Environment variable not set."));

        let msgs: Vec<Message> = if matches!(provider, Provider::Mistral) {
            vec![]
        } else {
            panic!()
        };

        Self {
            provider,
            model,
            token,
            temperature: Some(0.0),
            top_p: Some(1.0),
            max_tokens: Some(1028),
            prompt: String::new(),
            suffix: String::new(),
            messages: msgs,
        }
    }

    pub const fn temperature(mut self, temperature: Option<f32>) -> Self {
        if let Some(temperature) = temperature {
            self.temperature = Some(temperature);
        }
        self
    }

    pub const fn top_p(mut self, top_p: Option<f32>) -> Self {
        if let Some(top_p) = top_p {
            self.top_p = Some(top_p);
        }
        self
    }

    pub const fn max_tokens(mut self, max_tokens: Option<u32>) -> Self {
        if let Some(max_tokens) = max_tokens {
            self.max_tokens = Some(max_tokens);
        }
        self
    }

    pub async fn send_message(
        &mut self,
        message: &str,
        suffix: Option<String>,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        self.messages.push(Message {
            role: Role::User,
            content: message.to_string(),
        });

        message.clone_into(&mut self.prompt);
        if let Some(sfx) = &suffix {
            self.suffix.clone_from(sfx);
        }

        let prompt = if matches!(&self.provider, Provider::Mistral) {
            let mut json_map = serde_json::Map::new();
            json_map.insert("model".to_string(), json!(self.model));
            json_map.insert("temperature".to_string(), json!(self.temperature));
            json_map.insert("top_p".to_string(), json!(self.top_p));
            json_map.insert("max_tokens".to_string(), json!(self.max_tokens));
            json_map.insert("prompt".to_string(), json!(self.prompt));
            json_map.insert("suffix".to_string(), json!(self.suffix));
            json_map.insert("stop".to_string(), json!(["\n\n"]));
            json!(json_map)
        } else {
            panic!()
        };

        let request_url = if matches!(&self.provider, Provider::Mistral) {
            "https://codestral.mistral.ai/v1/fim/completions"
        } else {
            panic!()
        };

        let req_base = Client::new()
            .post(request_url)
            .json(&prompt)
            .header("content-type", "application/json");

        let req = if matches!(&self.provider, Provider::Mistral) {
            req_base.bearer_auth(self.token.to_string())
        } else {
            panic!()
        };

        let response = req.send().await?;

        if response.status().is_success() {
            let message = if matches!(&self.provider, Provider::Mistral) {
                let anth_response = response.json::<MistralResponse>().await?;
                anth_response.into_message()
            } else {
                panic!()
            };

            if let Some(msg) = message.clone() {
                self.messages.push(msg);
            }

            Ok(message)
        } else {
            match response.json::<Value>().await {
                Ok(resp_json) => match serde_json::to_string_pretty(&resp_json) {
                    Ok(resp_formatted) => {
                        Err(format!("{}\n\n{}", self.model, resp_formatted).into())
                    }
                    Err(e) => Err(format!("Failed to format response JSON: {e}").into()),
                },
                Err(e) => Err(format!("Failed to parse response JSON: {e}").into()),
            }
        }
    }

    pub fn get_message_history(&self) -> Vec<Message> {
        let msgs = self.messages.clone();
        if matches!(self.provider, Provider::Mistral) {
            msgs
        } else {
            panic!()
        }
    }
}
