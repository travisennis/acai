use crate::messages::Message;

// Define a trait named `LLMClient` with a generic type `R`.
pub trait LLMClient<R> {
    // Define an asynchronous method `send_message` that takes a slice of `Message` and returns a `Result` with type `R` or `reqwest::Error`.
    async fn send_message(&self, messages: &[Message]) -> Result<R, reqwest::Error>;
}
