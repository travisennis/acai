use serde::{Deserialize, Serialize};

use crate::models::{IntoMessage, Message};

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub choices: Vec<Choice>,
}

impl IntoMessage for Response {
    fn into_message(self) -> Option<Message> {
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
