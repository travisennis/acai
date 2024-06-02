use crate::models::Message;

/// Define a trait named `Response`.
pub trait Response {
    /// Define a method `get_message` that returns an optional `Message`.
    fn get_message(&self) -> Option<Message>;
}
