use serde::{Deserialize, Serialize};

use crate::models::{Message, Role};

use super::response::Response;

#[derive(Serialize, Deserialize, Debug)]
pub struct AnthropicResponse {
    pub role: Role,
    pub content: Vec<Content>,
}

impl Response for AnthropicResponse {
    fn get_message(&self) -> Option<Message> {
        if let Some(content) = self.content.first() {
            let msg = Message {
                role: self.role,
                content: content.text.to_string(),
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
