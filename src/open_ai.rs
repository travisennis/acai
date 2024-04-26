use std::env;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{clients::LLMClient, messages::Message, models::Model};

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub choices: Vec<Choice>,
    created: i64, // Using i64 assuming timestamp is in seconds
    id: String,
    model: String,
    object: String,
    usage: Usage,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Choice {
    finish_reason: String,
    index: i32,
    pub message: Message,
    logprobs: Option<f32>,
}

impl Choice {
    pub fn get_message(&self) -> Message {
        self.message.clone()
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Usage {
    completion_tokens: i32,
    prompt_tokens: i32,
    total_tokens: i32,
}

pub struct OpenAIApi {
    pub model: Model,
    pub temperature: f32,
}

impl LLMClient<Response> for OpenAIApi {
    async fn send_message(&self, messages: &[Message]) -> Result<Response, reqwest::Error> {
        let token = match env::var("OPENAI_API_KEY") {
            Ok(val) => val,
            Err(_) => {
                eprintln!("Error: Environment variable 'OPENAI_API_KEY' not set");
                std::process::exit(1);
            }
        };

        let prompt = json!({
            "model": &self.model,
            "temperature": &self.temperature,
            "messages": messages
        });

        let request_url = "https://api.openai.com/v1/chat/completions";
        let response = Client::new()
            .post(request_url)
            .bearer_auth(&token)
            .json(&prompt)
            .send()
            .await?
            .json::<Response>()
            .await?;

        Ok(response)
    }
}
