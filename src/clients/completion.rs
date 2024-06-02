use crate::clients::mistral::MistralResponse;
use crate::clients::response::Response;
use core::panic;
use std::{env, error::Error};

use reqwest::Client;
use serde_json::{json, Value};

use crate::models::{Message, Role};

use super::providers::{Model, Provider};

#[allow(clippy::module_name_repetitions)]
pub struct CompletionClient {
    provider: Provider,
    model: Model,
    token: String,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    prompt: String,
    suffix: String,
    messages: Vec<Message>,
}

impl CompletionClient {
    pub fn new(provider: Provider, model: Model) -> Self {
        let token = match provider {
            Provider::Mistral => env::var("MISTRAL_API_KEY"),
            _ => todo!(),
        }
        .unwrap_or_else(|_error| panic!("Error: Environment variable not set."));

        let msgs: Vec<Message> = match provider {
            Provider::Mistral => vec![],
            _ => panic!(),
        };

        CompletionClient {
            provider,
            model,
            token,
            temperature: Some(0.0),
            max_tokens: Some(1028),
            prompt: "".to_string(),
            suffix: "".to_string(),
            messages: msgs,
        }
    }

    pub fn temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub async fn send_message(
        &mut self,
        message: &str,
    ) -> Result<Option<Message>, Box<dyn Error + Send + Sync>> {
        self.messages.push(Message {
            role: Role::User,
            content: message.to_string(),
        });

        self.prompt = message.to_owned();

        let prompt = match &self.provider {
            Provider::Mistral => json!({
                "model": self.model,
                "temperature": self.temperature,
                "max_tokens": self.max_tokens,
                "prompt": self.prompt,
                "suffix": self.suffix,
            }),
            _ => panic!(),
        };

        let request_url = match &self.provider {
            Provider::Mistral => "https://codestral.mistral.ai/v1/fim/completions",
            _ => panic!(),
        };

        let req_base = Client::new()
            .post(request_url)
            .json(&prompt)
            .header("content-type", "application/json");
        let req = match &self.provider {
            Provider::Mistral => req_base.bearer_auth(self.token.to_string()),
            _ => panic!(),
        };

        let response = req.send().await?;

        if response.status().is_success() {
            let message = match &self.provider {
                Provider::Mistral => {
                    let anth_response = response.json::<MistralResponse>().await?;
                    anth_response.get_message()
                }
                _ => panic!(),
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
        match self.provider {
            Provider::Mistral => msgs,
            _ => panic!(),
        }
    }
}
