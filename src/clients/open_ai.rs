use serde::{Deserialize, Serialize};

use crate::models::Message;

use super::response::Response;

#[derive(Serialize, Deserialize, Debug)]
pub struct OpenAIResponse {
    pub choices: Vec<Choice>,
}

impl Response for OpenAIResponse {
    fn get_message(&self) -> Option<Message> {
        if let Some(choice) = self.choices.first() {
            let msg = choice.message.clone();
            return Some(msg);
        }
        None
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Choice {
    pub message: Message,
}
