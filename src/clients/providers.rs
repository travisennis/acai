pub enum Provider {
    /// Anthropic, provider of the Claude family of language models
    Anthropic,
    /// `OpenAI`, provider of GPT models including `ChatGPT`
    OpenAI,
    /// Mistral AI, provider of open-source language models
    Mistral,
    /// Google, provider of various AI models including `PaLM` and Gemini
    Google,
    /// Ollama, an open-source platform for running language models locally
    Ollama,
}

pub struct ModelConfig {
    /// The provider or source of the LLM (e.g., `OpenAI`, Google, etc.)
    pub provider: Provider,
    /// The name or identifier of the LLM (Large Language Model) being used
    pub model: String,
}

impl ModelConfig {
    /// Parses and constructs a `ModelConfig` from an input string.
    ///
    /// This function takes an input string in the format "provider/model" and returns a `ModelConfig`.
    /// If the input doesn't match the expected format, it uses default values.
    ///
    /// # Arguments
    ///
    /// * `input` - A string slice that should contain the provider and model name separated by a '/'.
    /// * `default` - A tuple containing the default Provider and model name as a string slice.
    ///
    /// # Returns
    ///
    /// Returns a `ModelConfig` struct with the parsed or default provider and model.
    pub fn get_or_default(input: &str, default: (Provider, &str)) -> Self {
        let parts: Vec<&str> = input.split('/').collect();

        let (provider_str, model_str) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
            ("", default.1)
        };

        let provider = match provider_str.to_lowercase().as_str() {
            "anthropic" => Provider::Anthropic,
            "openai" => Provider::OpenAI,
            "mistral" => Provider::Mistral,
            "google" => Provider::Google,
            "ollama" => Provider::Ollama,
            _ => default.0,
        };

        let model = match model_str.to_lowercase().as_str() {
            "gpt-4o" | "gpt4o" => "gpt-4o".into(),
            "gpt-4-turbo" | "gtp4turbo" => "gpt-4-turbo-preview".into(),
            "gpt-3.5-turbo" | "gpt35turbo" => "gpt-3.5-turbo".into(),
            "opus" => "claude-3-opus-20240229".into(),
            "sonnet" => "claude-3-5-sonnet-20240620".into(),
            "sonnet3" => "claude-3-sonnet-20240229".into(),
            "haiku" => "claude-3-haiku-20240307".into(),
            "codestral" => "codestral-latest".into(),
            "gemini-flash" => "gemini-1.5-flash-latest".into(),
            "gemini-pro" => "gemini-1.5-pro-latest".into(),
            _ => model_str.into(),
        };

        Self { provider, model }
    }
}
