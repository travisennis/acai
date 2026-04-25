use serde::{Deserialize, Serialize};

/// The type of API endpoint to use for model completions.
///
/// Cake supports multiple API backends for interacting with AI providers:
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
///
/// Contains all settings needed to connect to an AI model API, including
/// the model identifier, API endpoint, authentication, and generation parameters.
///
/// `ModelConfig` has no `Default` impl — it must be constructed from a
/// [`ModelDefinition`](crate::config::ModelDefinition) loaded from `settings.toml`.
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
    /// Reasoning effort level (none, low, medium, high, xhigh)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Reasoning summary mode (concise, detailed, auto) - Responses API only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_summary: Option<String>,
    /// Maximum reasoning tokens budget (for budget-style reasoning)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_max_tokens: Option<u32>,
    /// Provider routing hints
    pub providers: Vec<String>,
}

/// A `ModelConfig` with the API key resolved from the environment.
///
/// This struct is created by calling [`ResolvedModelConfig::resolve`] and contains
/// the actual API key value needed to make authenticated requests.
///
/// # Examples
///
/// ```no_run
/// use cake::config::{ModelConfig, ApiType, ResolvedModelConfig};
///
/// let config = ModelConfig {
///     model: "test/model".to_string(),
///     api_type: ApiType::ChatCompletions,
///     base_url: "https://api.example.com".to_string(),
///     api_key_env: "MY_KEY".to_string(),
///     temperature: None,
///     top_p: None,
///     max_output_tokens: None,
///     reasoning_effort: None,
///     reasoning_summary: None,
///     reasoning_max_tokens: None,
///     providers: vec![],
/// };
/// let resolved = ResolvedModelConfig::resolve(config)?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct ResolvedModelConfig {
    /// The underlying model configuration
    pub config: ModelConfig,
    /// The resolved API key value
    pub api_key: String,
}

impl ResolvedModelConfig {
    /// Resolves a `ModelConfig` by reading the API key from the environment.
    ///
    /// Reads the environment variable specified in `config.api_key_env` and
    /// returns a `ResolvedModelConfig` with the API key value.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use cake::config::{ModelConfig, ApiType, ResolvedModelConfig};
    ///
    /// let config = ModelConfig {
    ///     model: "test/model".to_string(),
    ///     api_type: ApiType::ChatCompletions,
    ///     base_url: "https://api.example.com".to_string(),
    ///     api_key_env: "CAKE_TEST_KEY".to_string(),
    ///     temperature: None,
    ///     top_p: None,
    ///     max_output_tokens: None,
    ///     reasoning_effort: None,
    ///     reasoning_summary: None,
    ///     reasoning_max_tokens: None,
    ///     providers: vec![],
    /// };
    /// let resolved = ResolvedModelConfig::resolve(config)?;
    /// println!("Using API key from: {}", resolved.config.api_key_env);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
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

    /// Helper to create a minimal `ModelConfig` for tests.
    fn test_config(overrides: impl FnOnce(&mut ModelConfig)) -> ModelConfig {
        let mut config = ModelConfig {
            model: "test/model".to_string(),
            api_type: ApiType::ChatCompletions,
            base_url: "https://api.example.com".to_string(),
            api_key_env: "MY_KEY".to_string(),
            temperature: Some(0.8),
            top_p: None,
            max_output_tokens: Some(8000),
            reasoning_effort: None,
            reasoning_summary: None,
            reasoning_max_tokens: None,
            providers: vec![],
        };
        overrides(&mut config);
        config
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
        let config = test_config(|_| {});
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ModelConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.model, config.model);
        assert_eq!(deserialized.api_type, config.api_type);
        assert_eq!(deserialized.base_url, config.base_url);
    }

    #[test]
    fn test_resolve_missing_env_var() {
        temp_env::with_var("CAKE_TEST_NONEXISTENT_KEY_12345", None::<&str>, || {
            let config = test_config(|c| {
                c.api_key_env = "CAKE_TEST_NONEXISTENT_KEY_12345".to_string();
            });

            let result = ResolvedModelConfig::resolve(config);
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("CAKE_TEST_NONEXISTENT_KEY_12345"));
        });
    }

    #[test]
    fn test_resolve_empty_env_var() {
        temp_env::with_var("CAKE_TEST_EMPTY_KEY", Some(""), || {
            let config = test_config(|c| {
                c.api_key_env = "CAKE_TEST_EMPTY_KEY".to_string();
            });

            let result = ResolvedModelConfig::resolve(config);
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("empty"));
        });
    }

    #[test]
    fn test_resolve_success() {
        temp_env::with_var("CAKE_TEST_VALID_KEY", Some("sk-test-123"), || {
            let config = test_config(|c| {
                c.api_key_env = "CAKE_TEST_VALID_KEY".to_string();
            });

            let resolved = ResolvedModelConfig::resolve(config).unwrap();
            assert_eq!(resolved.api_key, "sk-test-123");
            assert_eq!(resolved.config.api_key_env, "CAKE_TEST_VALID_KEY");
        });
    }
}
