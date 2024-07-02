pub enum Provider {
    Anthropic,
    OpenAI,
    Mistral,
    Google,
}

pub struct ModelConfig {
    pub provider: Provider,
    pub model: String,
}

impl ModelConfig {
    pub fn get_or_default(input: &str, default: (Provider, &str)) -> Self {
        let parts: Vec<&str> = input.split('/').collect();

        let p_m: (&str, &str) = if parts.len() != 2 {
            ("", default.1)
        } else {
            (parts[0], parts[1])
        };

        let provider = match p_m.0.to_lowercase().as_str() {
            "anthropic" => Provider::Anthropic,
            "openai" => Provider::OpenAI,
            "mistral" => Provider::Mistral,
            "google" => Provider::Google,
            _ => default.0,
        };

        let model = match p_m.1 {
            "gpt4o" => "gpt-4o".into(),
            "gtp4turbo" => "gpt-4-turbo-preview".into(),
            "gpt35turbo" => "gpt-3.5-turbo".into(),
            "opus" => "claude-3-opus-20240229".into(),
            "sonnet" => "claude-3-5-sonnet-20240620".into(),
            "sonnet3" => "claude-3-sonnet-20240229".into(),
            "haiku" => "claude-3-haiku-20240307".into(),
            "codestral" => "codestral-latest".into(),
            "gemini-flash" => "gemini-1.5-flash-latest".into(),
            "gemini-pro" => "gemini-1.5-pro-latest".into(),
            _ => parts[1].into(),
        };

        Self { provider, model }
    }
}
