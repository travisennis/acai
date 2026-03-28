use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::defaults::{DEFAULT_API_KEY_ENV, DEFAULT_BASE_URL, DEFAULT_PROVIDERS};
use crate::config::model::{ApiType, ModelConfig};

/// Root settings structure loaded from settings.toml
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// List of model definitions
    pub models: Vec<ModelDefinition>,
}

/// Definition of a named model in settings.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    /// Unique name for the model (lowercase alphanumeric + hyphens only)
    pub name: String,
    /// Model identifier (e.g. "glm-5", "anthropic/claude-3-sonnet")
    pub model: String,
    /// Base URL for the API endpoint
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Name of the environment variable containing the API key
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    /// Which API format to use
    #[serde(default)]
    pub api_type: ApiType,
    /// Sampling temperature
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Nucleus sampling parameter
    #[serde(default)]
    pub top_p: Option<f32>,
    /// Maximum number of output tokens
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    /// Reasoning effort level
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Reasoning summary mode (Responses API only)
    #[serde(default)]
    pub reasoning_summary: Option<String>,
    /// Maximum reasoning tokens budget
    #[serde(default)]
    pub reasoning_max_tokens: Option<u32>,
    /// Whether to exclude reasoning output from display
    #[serde(default)]
    pub reasoning_exclude: Option<bool>,
    /// Provider routing hints
    #[serde(default = "default_providers")]
    pub providers: Vec<String>,
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

fn default_api_key_env() -> String {
    DEFAULT_API_KEY_ENV.to_string()
}

fn default_providers() -> Vec<String> {
    DEFAULT_PROVIDERS.iter().map(|s| (*s).to_string()).collect()
}

impl ModelDefinition {
    /// Validate the model name (lowercase alphanumeric + hyphens only)
    pub fn validate_name(name: &str) -> Result<(), SettingsError> {
        if name.is_empty() {
            return Err(SettingsError::InvalidModelName {
                name: name.to_string(),
                reason: "name cannot be empty".to_string(),
            });
        }

        let valid_chars = name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');

        if !valid_chars {
            return Err(SettingsError::InvalidModelName {
                name: name.to_string(),
                reason: "must contain only lowercase letters, numbers, and hyphens".to_string(),
            });
        }

        Ok(())
    }

    /// Convert a `ModelDefinition` to a `ModelConfig`
    pub fn to_model_config(&self) -> ModelConfig {
        ModelConfig {
            model: self.model.clone(),
            api_type: self.api_type,
            base_url: self.base_url.clone(),
            api_key_env: self.api_key_env.clone(),
            temperature: self.temperature,
            top_p: self.top_p,
            max_output_tokens: self.max_output_tokens,
            reasoning_effort: self.reasoning_effort.clone(),
            reasoning_summary: self.reasoning_summary.clone(),
            reasoning_max_tokens: self.reasoning_max_tokens,
            reasoning_exclude: self.reasoning_exclude,
            providers: self.providers.clone(),
        }
    }
}

/// Errors that can occur when loading or processing settings
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("Invalid model name '{name}': {reason}")]
    InvalidModelName { name: String, reason: String },

    #[error("Duplicate model name '{name}' in settings")]
    DuplicateModelName { name: String },

    #[error("Failed to parse settings file: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Failed to read settings file: {0}")]
    IoError(#[from] std::io::Error),
}

/// Loader for settings from TOML files
pub struct SettingsLoader;

impl SettingsLoader {
    /// Load settings from a TOML file at the given path.
    /// Returns Ok(None) if the file doesn't exist.
    /// Returns an error if the file exists but is invalid.
    fn load_file(path: &Path) -> Result<Option<Settings>, SettingsError> {
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(path)?;
        let settings: Settings = toml::from_str(&content)?;
        Ok(Some(settings))
    }

    /// Load and merge settings from multiple locations.
    ///
    /// Merge logic:
    /// - Start with global settings
    /// - Add/override with project settings (project takes precedence for same name)
    /// - Check for duplicate names within each file (error if found)
    ///
    /// Returns a map of model name -> `ModelDefinition`.
    pub fn load(
        project_dir: Option<&Path>,
        global_dir: &Path,
    ) -> Result<HashMap<String, ModelDefinition>, SettingsError> {
        let mut models: HashMap<String, ModelDefinition> = HashMap::new();

        // Load global settings first
        let global_path = global_dir.join("settings.toml");
        if let Some(settings) = Self::load_file(&global_path)? {
            Self::add_models_to_map(&mut models, settings.models)?;
        }

        // Load project settings (override global for same names)
        if let Some(project_dir) = project_dir {
            let project_path = project_dir.join(".acai").join("settings.toml");
            if let Some(settings) = Self::load_file(&project_path)? {
                Self::add_models_to_map(&mut models, settings.models)?;
            }
        }

        Ok(models)
    }

    /// Add models from a settings file to the map.
    ///
    /// Checks for duplicate names within the same file (errors if found).
    /// Allows overriding models from previous files (e.g., global settings).
    fn add_models_to_map(
        map: &mut HashMap<String, ModelDefinition>,
        definitions: Vec<ModelDefinition>,
    ) -> Result<(), SettingsError> {
        // First, check for duplicates within the same file
        let mut seen: HashSet<&str> = HashSet::new();
        for def in &definitions {
            // Validate name format
            if let Err(e) = ModelDefinition::validate_name(&def.name) {
                return Err(SettingsError::InvalidModelName {
                    name: def.name.clone(),
                    reason: e.to_string(),
                });
            }

            // Check for duplicates within this file
            if !seen.insert(def.name.as_str()) {
                return Err(SettingsError::DuplicateModelName {
                    name: def.name.clone(),
                });
            }
        }

        // Now add all models to the map (overwriting any existing entries)
        for def in definitions {
            let name = def.name.clone();
            map.insert(name, def);
        }

        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a temp directory with settings.toml at the root (for global settings)
    fn create_global_settings(content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, content).unwrap();
        dir
    }

    /// Create a temp directory with .acai/settings.toml (for project settings)
    fn create_project_settings(content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let acai_dir = dir.path().join(".acai");
        std::fs::create_dir_all(&acai_dir).unwrap();
        let path = acai_dir.join("settings.toml");
        std::fs::write(&path, content).unwrap();
        dir
    }

    #[test]
    fn test_load_single_file() {
        let dir = create_project_settings(
            r#"
[[models]]
name = "test-model"
model = "test/model"
"#,
        );

        let models = SettingsLoader::load(Some(dir.path()), Path::new("/nonexistent")).unwrap();

        assert_eq!(models.len(), 1);
        assert!(models.contains_key("test-model"));
        assert_eq!(models.get("test-model").unwrap().model, "test/model");
    }

    #[test]
    fn test_load_merges_with_override() {
        // Global has "model-a" and "model-b"
        let global_dir = create_global_settings(
            r#"
[[models]]
name = "model-a"
model = "global/model-a"

[[models]]
name = "model-b"
model = "global/model-b"
"#,
        );

        // Project has "model-b" (override) and "model-c" (new)
        let project_dir = create_project_settings(
            r#"
[[models]]
name = "model-b"
model = "project/model-b"

[[models]]
name = "model-c"
model = "project/model-c"
"#,
        );

        let models = SettingsLoader::load(Some(project_dir.path()), global_dir.path()).unwrap();

        assert_eq!(models.len(), 3);
        // model-a from global
        assert_eq!(models.get("model-a").unwrap().model, "global/model-a");
        // model-b overridden by project
        assert_eq!(models.get("model-b").unwrap().model, "project/model-b");
        // model-c from project
        assert_eq!(models.get("model-c").unwrap().model, "project/model-c");
    }

    #[test]
    fn test_load_missing_file_succeeds() {
        let models = SettingsLoader::load(
            Some(Path::new("/nonexistent")),
            Path::new("/also/nonexistent"),
        );
        assert!(models.is_ok());
        assert!(models.unwrap().is_empty());
    }

    #[test]
    fn test_duplicate_name_in_file() {
        let dir = create_project_settings(
            r#"
[[models]]
name = "dup"
model = "first"

[[models]]
name = "dup"
model = "second"
"#,
        );

        let result = SettingsLoader::load(Some(dir.path()), Path::new("/nonexistent"));
        assert!(matches!(result, Err(SettingsError::DuplicateModelName { name }) if name == "dup"));
    }

    #[test]
    fn test_invalid_name_format() {
        let dir = create_project_settings(
            r#"
[[models]]
name = "Invalid Name!"
model = "test"
"#,
        );

        let result = SettingsLoader::load(Some(dir.path()), Path::new("/nonexistent"));
        assert!(matches!(
            result,
            Err(SettingsError::InvalidModelName { name, .. }) if name == "Invalid Name!"
        ));
    }

    #[test]
    fn test_model_definition_defaults() {
        let dir = create_project_settings(
            r#"
[[models]]
name = "minimal"
model = "test/model"
"#,
        );

        let models = SettingsLoader::load(Some(dir.path()), Path::new("/nonexistent")).unwrap();
        let def = models.get("minimal").unwrap();

        assert_eq!(def.base_url, DEFAULT_BASE_URL);
        assert_eq!(def.api_key_env, DEFAULT_API_KEY_ENV);
        assert_eq!(def.api_type, ApiType::ChatCompletions);
        assert!(def.providers.is_empty());
        assert_eq!(def.reasoning_effort, None);
        assert_eq!(def.reasoning_summary, None);
        assert_eq!(def.reasoning_max_tokens, None);
        assert_eq!(def.reasoning_exclude, None);
    }

    #[test]
    fn test_validate_name_valid() {
        assert!(ModelDefinition::validate_name("simple").is_ok());
        assert!(ModelDefinition::validate_name("my-model").is_ok());
        assert!(ModelDefinition::validate_name("model-123").is_ok());
        assert!(ModelDefinition::validate_name("a").is_ok());
        assert!(ModelDefinition::validate_name("a1b2c3").is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        assert!(ModelDefinition::validate_name("").is_err());
        assert!(ModelDefinition::validate_name("Invalid").is_err());
        assert!(ModelDefinition::validate_name("my_model").is_err());
        assert!(ModelDefinition::validate_name("model.123").is_err());
        assert!(ModelDefinition::validate_name("model 123").is_err());
    }

    #[test]
    fn test_to_model_config() {
        let def = ModelDefinition {
            name: "test".to_string(),
            model: "test/model".to_string(),
            base_url: "https://example.com".to_string(),
            api_key_env: "MY_KEY".to_string(),
            api_type: ApiType::Responses,
            temperature: Some(0.5),
            top_p: Some(0.9),
            max_output_tokens: Some(4000),
            reasoning_effort: Some("high".to_string()),
            reasoning_summary: Some("concise".to_string()),
            reasoning_max_tokens: Some(8000),
            reasoning_exclude: Some(false),
            providers: vec!["Provider1".to_string()],
        };

        let config = def.to_model_config();

        assert_eq!(config.model, "test/model");
        assert_eq!(config.base_url, "https://example.com");
        assert_eq!(config.api_key_env, "MY_KEY");
        assert_eq!(config.api_type, ApiType::Responses);
        assert_eq!(config.temperature, Some(0.5));
        assert_eq!(config.top_p, Some(0.9));
        assert_eq!(config.max_output_tokens, Some(4000));
        assert_eq!(config.reasoning_effort, Some("high".to_string()));
        assert_eq!(config.reasoning_summary, Some("concise".to_string()));
        assert_eq!(config.reasoning_max_tokens, Some(8000));
        assert_eq!(config.reasoning_exclude, Some(false));
        assert_eq!(config.providers, vec!["Provider1"]);
    }
}
