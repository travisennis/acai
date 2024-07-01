use serde::{Deserialize, Serialize};

use crate::models::{IntoMessage, Message, Role};

#[derive(Serialize, Deserialize, Debug)]
pub struct Part {
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SystemInstruction {
    pub parts: Part,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Instruction {
    pub role: String,
    pub parts: Vec<Part>,
}

impl From<&Message> for Instruction {
    fn from(value: &Message) -> Self {
        let role = match value.role {
            crate::models::Role::System => "system".to_string(),
            crate::models::Role::Assistant => "assistant".to_string(),
            crate::models::Role::User => "user".to_string(),
        };

        Self {
            role,
            parts: vec![Part {
                text: value.content.clone(),
            }],
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Request {
    pub system_instruction: SystemInstruction,
    pub contents: Vec<Instruction>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Content {
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub contents: Vec<Content>,
}

impl IntoMessage for Response {
    fn into_message(self) -> Option<Message> {
        if let Some(content) = self.contents.first() {
            if let Some(part) = content.parts.first() {
                return Some(Message {
                    role: Role::Assistant,
                    content: part.text.clone(),
                });
            }
        }
        None
    }
}
