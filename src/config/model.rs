use serde::{Deserialize, Serialize};

use crate::config::defaults::{
    DEFAULT_API_KEY_ENV, DEFAULT_BASE_URL, DEFAULT_MODEL, DEFAULT_PROVIDERS,
};

/// The type of API endpoint to use for model completions.
///
/// Acai supports multiple API backends for interacting with AI providers:
///
/// - `Responses`: `OpenRouter`'s Responses API format, which supports reasoning traces
///   and structured outputs. Use this for providers that support the Responses API.
///
/// - `ChatCompletions`: The standard OpenAI-compatible Chat Completions format, which
///   is widely supported by most AI providers. Use this for maximum compatibility.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApiType {
    /// OpenAI-compatible Chat Completions API - widely supported by most providers
    #[default]
    ChatCompletions,
    /// `OpenRouter` Responses API - supports reasoning traces and structured outputs
    Responses,
}

/// Configuration for a model provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Model identifier (e.g. "openai/gpt-4o")
    pub model: String,
    /// Which API format to use
    pub api_type: ApiType,
    /// Base URL for the API endpoint
    pub base_url: String,
    /// Name of the environment variable containing the API key
    pub api_key_env: String,
    /// Sampling temperature
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Maximum number of output tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// Provider routing hints
    pub providers: Vec<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            api_type: ApiType::ChatCompletions,
            base_url: DEFAULT_BASE_URL.to_string(),
            api_key_env: DEFAULT_API_KEY_ENV.to_string(),
            temperature: Some(0.8),
            top_p: None,
            max_output_tokens: Some(8000),
            providers: DEFAULT_PROVIDERS.iter().map(|s| (*s).to_string()).collect(),
        }
    }
}

/// A `ModelConfig` with the API key resolved from the environment.
#[derive(Debug, Clone)]
pub struct ResolvedModelConfig {
    /// The underlying model configuration
    pub config: ModelConfig,
    /// The resolved API key value
    pub api_key: String,
}

impl ResolvedModelConfig {
    /// Resolve a `ModelConfig` by reading the API key from the environment.
    ///
    /// # Errors
    ///
    /// Returns an error if the environment variable named in
    /// `config.api_key_env` is not set or is empty.
    pub fn resolve(config: ModelConfig) -> anyhow::Result<Self> {
        let api_key = std::env::var(&config.api_key_env).map_err(|err| {
            anyhow::anyhow!(
                "Environment variable '{}' is not set. Please set it to your API key: {err}",
                config.api_key_env
            )
        })?;

        anyhow::ensure!(
            !api_key.is_empty(),
            "Environment variable '{}' is set but empty",
            config.api_key_env
        );

        Ok(Self { config, api_key })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn test_default_model_config() {
        let config = ModelConfig::default();
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.api_type, ApiType::ChatCompletions);
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert_eq!(config.api_key_env, DEFAULT_API_KEY_ENV);
        assert_eq!(config.temperature, Some(0.8));
        assert_eq!(config.top_p, None);
        assert_eq!(config.max_output_tokens, Some(8000));
    }

    #[test]
    fn test_api_type_serialization() {
        let json = serde_json::to_string(&ApiType::Responses).unwrap();
        assert_eq!(json, r#""responses""#);

        let json = serde_json::to_string(&ApiType::ChatCompletions).unwrap();
        assert_eq!(json, r#""chat_completions""#);
    }

    #[test]
    fn test_model_config_roundtrip() {
        let config = ModelConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model, config.model);
        assert_eq!(deserialized.api_type, config.api_type);
        assert_eq!(deserialized.base_url, config.base_url);
    }

    #[test]
    fn test_resolve_missing_env_var() {
        temp_env::with_var("ACAI_TEST_NONEXISTENT_KEY_12345", None::<&str>, || {
            let config = ModelConfig {
                api_key_env: "ACAI_TEST_NONEXISTENT_KEY_12345".to_string(),
                ..ModelConfig::default()
            };

            let result = ResolvedModelConfig::resolve(config);
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("ACAI_TEST_NONEXISTENT_KEY_12345"));
        });
    }

    #[test]
    fn test_resolve_empty_env_var() {
        temp_env::with_var("ACAI_TEST_EMPTY_KEY", Some(""), || {
            let config = ModelConfig {
                api_key_env: "ACAI_TEST_EMPTY_KEY".to_string(),
                ..ModelConfig::default()
            };

            let result = ResolvedModelConfig::resolve(config);
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("empty"));
        });
    }

    #[test]
    fn test_resolve_success() {
        temp_env::with_var("ACAI_TEST_VALID_KEY", Some("sk-test-123"), || {
            let config = ModelConfig {
                api_key_env: "ACAI_TEST_VALID_KEY".to_string(),
                ..ModelConfig::default()
            };

            let resolved = ResolvedModelConfig::resolve(config).unwrap();
            assert_eq!(resolved.api_key, "sk-test-123");
            assert_eq!(resolved.config.api_key_env, "ACAI_TEST_VALID_KEY");
        });
    }
}
