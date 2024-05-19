use crate::clients::{Model, Provider};

pub struct CmdConfig {
    pub provider: Provider,
    pub model: Model,
    pub context: Option<String>,
    pub temperature: f32,
    pub top_p: f32,
    pub max_tokens: u32,
}

impl CmdConfig {
    pub fn new(
        model: &str,
        context: Option<String>,
        temperature: f32,
        top_p: f32,
        max_tokens: u32,
    ) -> Self {
        let model_provider = match model {
            "gpt-4-turbo" => (Provider::OpenAI, Model::GPT4Turbo),
            "gpt-3-turbo" => (Provider::OpenAI, Model::GPT3Turbo),
            "opus" => (Provider::Anthropic, Model::ClaudeOpus),
            "sonnet" => (Provider::Anthropic, Model::ClaudeSonnet),
            "haiku" => (Provider::Anthropic, Model::ClaudeHaiku),
            _ => (Provider::OpenAI, Model::GPT4o),
        };

        Self {
            provider: model_provider.0,
            model: model_provider.1,
            context,
            temperature,
            top_p,
            max_tokens,
        }
    }
}
