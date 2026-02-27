use serde::{Deserialize, Serialize};

use crate::models::{IntoMessage, Message, Role};

#[derive(Serialize)]
pub struct Request {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    pub max_tokens: u32,
    pub system: String,
    pub messages: Vec<Message>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // stop: Option<Vec<String>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // presence_penalty: Option<f32>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // frequency_penalty: Option<f32>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // logit_bias: Option<std::collections::HashMap<String, f32>>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    pub stream: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub role: Role,
    pub content: Vec<Content>,
}

impl IntoMessage for Response {
    fn into_message(self) -> Option<Message> {
        if let Some(content) = self.content.first() {
            let msg = Message {
                role: self.role,
                content: content.text.clone(),
            };
            return Some(msg);
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Content {
    text: String,
}
