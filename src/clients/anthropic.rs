use serde::{Deserialize, Serialize};

use crate::models::{IntoMessage, Message, Role};

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
