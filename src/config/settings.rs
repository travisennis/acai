use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::model::{ApiType, ModelConfig};
use crate::config::skills::SkillConfig;

/// Skill settings loaded from settings.toml.
///
/// Controls skill discovery and filtering behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillSettings {
    /// If true, disable all skills
    #[serde(default)]
    pub disabled: bool,
    /// Optional list of skill names to load (empty = all)
    #[serde(default)]
    pub only: Vec<String>,
}

/// Root settings structure loaded from settings.toml.
///
/// Contains a list of model definitions, an optional default model name,
/// and optional additional directories for read-write access.
///
/// # Examples
///
/// ```no_run
/// use cake::config::SettingsLoader;
///
/// let loaded = SettingsLoader::load(None)?;
/// for (name, def) in &loaded.models {
///     println!("Model: {} -> {}", name, def.model);
/// }
/// if let Some(default) = &loaded.default_model {
///     println!("Default model: {default}");
/// }
/// # Ok::<(), cake::config::SettingsError>(())
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Name of the model to use when `--model` is not specified
    #[serde(default)]
    pub default_model: Option<String>,
    /// List of model definitions
    #[serde(default)]
    pub models: Vec<ModelDefinition>,
    /// Skill configuration
    #[serde(default)]
    pub skills: SkillSettings,
    /// Additional directories for read-write access.
    /// Merged from global and project settings.
    #[serde(default)]
    pub directories: Vec<String>,
}

/// Result of loading and merging settings from all sources.
///
/// Contains the merged model map, the resolved default model name,
/// and merged directories.
/// Separate from [`Settings`] to represent the post-merge state.
#[derive(Debug, Clone)]
pub struct LoadedSettings {
    /// Map of model name to definition (global + project merged)
    pub models: HashMap<String, ModelDefinition>,
    /// Name of the default model from the highest-precedence settings file
    pub default_model: Option<String>,
    /// Additional directories for read-write access (global + project merged)
    pub directories: Vec<String>,
}

/// Definition of a named model in settings.toml.
///
/// Each model has a unique name that can be used with `--model <name>`
/// to select a specific model configuration.
///
/// # Examples
///
/// ```no_run
/// use cake::config::SettingsLoader;
///
/// let loaded = SettingsLoader::load(None)?;
/// if let Some(def) = loaded.models.get("my-model") {
///     println!("Using model: {}", def.model);
/// }
/// # Ok::<(), cake::config::SettingsError>(())
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDefinition {
    /// Unique name for the model (lowercase alphanumeric + hyphens only)
    pub name: String,
    /// Model identifier (e.g. "glm-5.1", "anthropic/claude-3-sonnet")
    pub model: String,
    /// Base URL for the API endpoint (required)
    pub base_url: String,
    /// Name of the environment variable containing the API key (required)
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
    /// Provider routing hints
    #[serde(default)]
    pub providers: Vec<String>,
}

impl ModelDefinition {
    /// Validates the model name format.
    ///
    /// Model names must be lowercase alphanumeric with hyphens only.
    ///
    /// # Examples
    ///
    /// ```
    /// use cake::config::ModelDefinition;
    ///
    /// assert!(ModelDefinition::validate_name("my-model").is_ok());
    /// assert!(ModelDefinition::validate_name("model-123").is_ok());
    /// assert!(ModelDefinition::validate_name("Invalid").is_err());
    /// assert!(ModelDefinition::validate_name("my_model").is_err());
    /// ```
    ///
    /// # Errors
    ///
    /// Returns `SettingsError::InvalidModelName` if the name is empty or
    /// contains invalid characters.
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

    /// Converts the model definition to a `ModelConfig`.
    ///
    /// # Examples
    ///
    /// ```
    /// use cake::config::{ModelDefinition, ApiType};
    ///
    /// let def = ModelDefinition {
    ///     name: "test".to_string(),
    ///     model: "test/model".to_string(),
    ///     base_url: "https://example.com".to_string(),
    ///     api_key_env: "MY_KEY".to_string(),
    ///     api_type: ApiType::ChatCompletions,
    ///     temperature: Some(0.5),
    ///     top_p: None,
    ///     max_output_tokens: Some(4000),
    ///     reasoning_effort: None,
    ///     reasoning_summary: None,
    ///     reasoning_max_tokens: None,
    ///     providers: vec![],
    /// };
    ///
    /// let config = def.to_model_config();
    /// assert_eq!(config.model, "test/model");
    /// ```
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

    #[error(
        "Default model '{name}' not found in models list. \
         Define a [[models]] entry with name = \"{name}\", \
         or change default_model to an existing model name."
    )]
    DefaultModelNotFound { name: String },

    #[error("Failed to parse settings file: {0}")]
    ParseError(#[from] toml::de::Error),

    #[error("Failed to read settings file: {0}")]
    IoError(#[from] std::io::Error),
}

/// Loader for settings from TOML files.
///
/// Settings are loaded from both global and project-level files,
/// with project settings taking precedence over global settings.
///
/// # Examples
///
/// ```no_run
/// use cake::config::SettingsLoader;
/// use std::path::Path;
///
/// let loaded = SettingsLoader::load(Some(Path::new("/project")))?;
///
/// if let Some(model) = loaded.models.get("zen") {
///     println!("Model: {}", model.model);
/// }
/// println!("Default: {:?}", loaded.default_model);
/// # Ok::<(), cake::config::SettingsError>(())
/// ```
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

    /// Load skill settings from global and project TOML files.
    ///
    /// Project settings override global settings.
    pub fn load_skill_settings(project_dir: Option<&Path>) -> SkillSettings {
        let mut settings = SkillSettings::default();

        // Load global settings first
        if let Some(home_dir) = dirs::home_dir() {
            let global_path = home_dir.join(".config").join("cake").join("settings.toml");
            if let Ok(Some(loaded)) = Self::load_file(&global_path) {
                settings = loaded.skills;
            }
        }

        // Load project settings last (override global)
        if let Some(project_dir) = project_dir {
            let project_path = project_dir.join(".cake").join("settings.toml");
            if let Ok(Some(loaded)) = Self::load_file(&project_path) {
                settings = loaded.skills;
            }
        }

        settings
    }

    /// Resolve skill configuration from CLI flags and settings.
    ///
    /// Precedence (highest to lowest):
    /// 1. `--no-skills` CLI flag
    /// 2. `--skills name1,name2` CLI flag
    /// 3. `skills.only` in settings
    /// 4. `skills.disabled = true` in settings
    /// 5. Default: load all discovered skills
    pub fn resolve_skill_config(
        no_skills: bool,
        skills_flag: Option<&str>,
        settings: &SkillSettings,
    ) -> SkillConfig {
        if no_skills {
            return SkillConfig::Disabled;
        }

        if let Some(names) = skills_flag {
            let skill_names: Vec<String> = names
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if skill_names.is_empty() {
                return SkillConfig::Disabled;
            }
            return SkillConfig::Only(skill_names);
        }

        if !settings.only.is_empty() {
            return SkillConfig::Only(settings.only.clone());
        }

        if settings.disabled {
            return SkillConfig::Disabled;
        }

        SkillConfig::All
    }

    /// Loads and merges settings from global and project locations.
    ///
    /// Settings are loaded from:
    /// 1. Global settings: `~/.config/cake/settings.toml`
    /// 2. Project settings: `{project_dir}/.cake/settings.toml`
    ///
    /// Project settings override global settings for models with the same name
    /// and for `default_model`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use cake::config::SettingsLoader;
    /// use std::path::Path;
    ///
    /// let loaded = SettingsLoader::load(Some(Path::new("/my/project")))?;
    ///
    /// if let Some(model) = loaded.models.get("default") {
    ///     println!("Default model: {}", model.model);
    /// }
    /// # Ok::<(), cake::config::SettingsError>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if a settings file exists but cannot be parsed,
    /// if duplicate model names are found within the same file,
    /// or if `default_model` references a model that doesn't exist.
    pub fn load(project_dir: Option<&Path>) -> Result<LoadedSettings, SettingsError> {
        let mut models: HashMap<String, ModelDefinition> = HashMap::new();
        let mut default_model: Option<String> = None;
        let mut directories: HashSet<String> = HashSet::new();

        // Load global settings first.
        if let Some(home_dir) = dirs::home_dir() {
            let global_path = home_dir.join(".config").join("cake").join("settings.toml");
            if let Some(settings) = Self::load_file(&global_path)? {
                Self::add_models_to_map(&mut models, settings.models)?;
                default_model = settings.default_model;
                for dir in settings.directories {
                    directories.insert(dir);
                }
            }
        }

        // Load project settings last so they override global settings.
        if let Some(project_dir) = project_dir {
            let project_path = project_dir.join(".cake").join("settings.toml");
            if let Some(settings) = Self::load_file(&project_path)? {
                Self::add_models_to_map(&mut models, settings.models)?;
                if settings.default_model.is_some() {
                    default_model = settings.default_model;
                }
                for dir in settings.directories {
                    directories.insert(dir);
                }
            }
        }

        // If project set default_model to None explicitly, respect that.
        // We handle this by checking project settings file directly for the field.
        if let Some(project_dir) = project_dir {
            let project_path = project_dir.join(".cake").join("settings.toml");
            if let Ok(Some(settings)) = Self::load_file(&project_path) {
                // Did the project file contain a default_model key at all?
                // Serde treats missing key as None, but we need to distinguish
                // "not set" from "explicitly set to an empty/default".
                // Read raw to check.
                if let Ok(content) = std::fs::read_to_string(&project_path) {
                    let raw: toml::Value = toml::from_str(&content)
                        .unwrap_or_else(|_| toml::Value::Table(toml::Table::new()));
                    if let Some(table) = raw.as_table()
                        && table.contains_key("default_model")
                    {
                        // Project file explicitly mentions default_model.
                        // If the parsed value is None, they explicitly cleared it.
                        if settings.default_model.is_none() {
                            default_model = None;
                        }
                    }
                }
            }
        }

        // Validate that default_model (if set) refers to an existing model.
        if let Some(ref name) = default_model
            && !models.contains_key(name.as_str())
        {
            return Err(SettingsError::DefaultModelNotFound { name: name.clone() });
        }

        Ok(LoadedSettings {
            models,
            default_model,
            directories: directories.into_iter().collect(),
        })
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
    use temp_env::with_var;
    use tempfile::TempDir;

    fn create_home_dir() -> TempDir {
        let home = TempDir::new().unwrap();
        std::fs::create_dir_all(home.path().join(".config")).unwrap();
        home
    }

    fn write_global_settings(home: &Path, content: &str) {
        let xdg_dir = home.join(".config").join("cake");
        std::fs::create_dir_all(&xdg_dir).unwrap();
        std::fs::write(xdg_dir.join("settings.toml"), content).unwrap();
    }

    /// Create a temp directory with .cake/settings.toml (for project settings)
    fn create_project_settings(content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let cake_dir = dir.path().join(".cake");
        std::fs::create_dir_all(&cake_dir).unwrap();
        let path = cake_dir.join("settings.toml");
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
base_url = "https://example.com"
api_key_env = "MY_KEY"
"#,
        );

        let home = create_home_dir();
        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(dir.path()))
        })
        .unwrap();

        assert_eq!(loaded.models.len(), 1);
        assert!(loaded.models.contains_key("test-model"));
        assert_eq!(loaded.models.get("test-model").unwrap().model, "test/model");
    }

    #[test]
    fn test_load_merges_with_override() {
        let home = create_home_dir();
        // Global has "model-a" and "model-b"
        write_global_settings(
            home.path(),
            r#"
[[models]]
name = "model-a"
model = "global/model-a"
base_url = "https://global.example.com"
api_key_env = "GLOBAL_KEY"

[[models]]
name = "model-b"
model = "global/model-b"
base_url = "https://global.example.com"
api_key_env = "GLOBAL_KEY"
"#,
        );

        // Project has "model-b" (override) and "model-c" (new)
        let project_dir = create_project_settings(
            r#"
[[models]]
name = "model-b"
model = "project/model-b"
base_url = "https://project.example.com"
api_key_env = "PROJECT_KEY"

[[models]]
name = "model-c"
model = "project/model-c"
base_url = "https://project.example.com"
api_key_env = "PROJECT_KEY"
"#,
        );

        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(project_dir.path()))
        })
        .unwrap();

        assert_eq!(loaded.models.len(), 3);
        // model-a from global
        assert_eq!(
            loaded.models.get("model-a").unwrap().model,
            "global/model-a"
        );
        // model-b overridden by project
        assert_eq!(
            loaded.models.get("model-b").unwrap().model,
            "project/model-b"
        );
        // model-c from project
        assert_eq!(
            loaded.models.get("model-c").unwrap().model,
            "project/model-c"
        );
    }

    #[test]
    fn test_load_reads_xdg_global_settings() {
        let home = create_home_dir();
        write_global_settings(
            home.path(),
            r#"
[[models]]
name = "xdg-model"
model = "xdg/model"
base_url = "https://example.com"
api_key_env = "XDG_KEY"
"#,
        );

        let loaded = with_var("HOME", Some(home.path()), || SettingsLoader::load(None)).unwrap();

        assert_eq!(loaded.models.len(), 1);
        assert_eq!(loaded.models.get("xdg-model").unwrap().model, "xdg/model");
    }

    #[test]
    fn test_project_overrides_xdg_global() {
        let home = create_home_dir();
        write_global_settings(
            home.path(),
            r#"
[[models]]
name = "shared"
model = "xdg/model"
base_url = "https://global.example.com"
api_key_env = "GLOBAL_KEY"
"#,
        );
        let project_dir = create_project_settings(
            r#"
[[models]]
name = "shared"
model = "project/model"
base_url = "https://project.example.com"
api_key_env = "PROJECT_KEY"
"#,
        );

        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(project_dir.path()))
        })
        .unwrap();

        assert_eq!(loaded.models.get("shared").unwrap().model, "project/model");
    }

    #[test]
    fn test_load_missing_file_succeeds() {
        let home = create_home_dir();
        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(Path::new("/nonexistent")))
        });
        assert!(loaded.is_ok());
        assert!(loaded.unwrap().models.is_empty());
    }

    #[test]
    fn test_duplicate_name_in_file() {
        let dir = create_project_settings(
            r#"
[[models]]
name = "dup"
model = "first"
base_url = "https://example.com"
api_key_env = "MY_KEY"

[[models]]
name = "dup"
model = "second"
base_url = "https://example.com"
api_key_env = "MY_KEY"
"#,
        );

        let home = create_home_dir();
        let result = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(dir.path()))
        });
        assert!(matches!(result, Err(SettingsError::DuplicateModelName { name }) if name == "dup"));
    }

    #[test]
    fn test_invalid_name_format() {
        let dir = create_project_settings(
            r#"
[[models]]
name = "Invalid Name!"
model = "test"
base_url = "https://example.com"
api_key_env = "MY_KEY"
"#,
        );

        let home = create_home_dir();
        let result = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(dir.path()))
        });
        assert!(matches!(
            result,
            Err(SettingsError::InvalidModelName { name, .. }) if name == "Invalid Name!"
        ));
    }

    #[test]
    fn test_model_definition_all_fields() {
        let dir = create_project_settings(
            r#"
[[models]]
name = "minimal"
model = "test/model"
base_url = "https://example.com"
api_key_env = "MY_KEY"
"#,
        );

        let home = create_home_dir();
        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(dir.path()))
        })
        .unwrap();
        let def = loaded.models.get("minimal").unwrap();

        assert_eq!(def.model, "test/model");
        assert_eq!(def.base_url, "https://example.com");
        assert_eq!(def.api_key_env, "MY_KEY");
        assert_eq!(def.api_type, ApiType::ChatCompletions);
        assert!(def.providers.is_empty());
        assert_eq!(def.reasoning_effort, None);
        assert_eq!(def.reasoning_summary, None);
        assert_eq!(def.reasoning_max_tokens, None);
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
        assert_eq!(config.providers, vec!["Provider1"]);
    }

    // --- LoadedSettings and default_model tests ---

    #[test]
    fn test_default_model_valid() {
        let dir = create_project_settings(
            r#"
default_model = "zen"

[[models]]
name = "zen"
model = "glm-5.1"
base_url = "https://opencode.ai/zen/go/v1/"
api_key_env = "OPENCODE_ZEN_API_TOKEN"
"#,
        );

        let home = create_home_dir();
        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(dir.path()))
        })
        .unwrap();

        assert_eq!(loaded.default_model, Some("zen".to_string()));
        assert!(loaded.models.contains_key("zen"));
    }

    #[test]
    fn test_default_model_not_found() {
        let dir = create_project_settings(
            r#"
default_model = "nonexistent"

[[models]]
name = "zen"
model = "glm-5.1"
base_url = "https://example.com"
api_key_env = "KEY"
"#,
        );

        let home = create_home_dir();
        let result = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(dir.path()))
        });
        assert!(matches!(
            result,
            Err(SettingsError::DefaultModelNotFound { name }) if name == "nonexistent"
        ));
    }

    #[test]
    fn test_no_default_model() {
        let dir = create_project_settings(
            r#"
[[models]]
name = "zen"
model = "glm-5.1"
base_url = "https://example.com"
api_key_env = "KEY"
"#,
        );

        let home = create_home_dir();
        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(dir.path()))
        })
        .unwrap();

        assert_eq!(loaded.default_model, None);
    }

    #[test]
    fn test_project_overrides_default_model() {
        let home = create_home_dir();
        write_global_settings(
            home.path(),
            r#"
default_model = "global-model"

[[models]]
name = "global-model"
model = "global/model"
base_url = "https://global.example.com"
api_key_env = "GLOBAL_KEY"
"#,
        );

        let project_dir = create_project_settings(
            r#"
default_model = "project-model"

[[models]]
name = "project-model"
model = "project/model"
base_url = "https://project.example.com"
api_key_env = "PROJECT_KEY"
"#,
        );

        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(project_dir.path()))
        })
        .unwrap();

        assert_eq!(loaded.default_model, Some("project-model".to_string()));
    }

    #[test]
    fn test_directories_merge_global_and_project() {
        let home = create_home_dir();
        write_global_settings(
            home.path(),
            r#"
directories = ["/global/dir1", "/global/dir2"]

[[models]]
name = "zen"
model = "glm-5.1"
base_url = "https://example.com"
api_key_env = "KEY"
"#,
        );

        let project_dir = create_project_settings(
            r#"
directories = ["/project/dir1", "/global/dir2"]

[[models]]
name = "proj"
model = "proj/model"
base_url = "https://project.example.com"
api_key_env = "PROJ_KEY"
"#,
        );

        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(project_dir.path()))
        })
        .unwrap();

        // Directories are merged without duplicates
        assert_eq!(loaded.directories.len(), 3);
        assert!(loaded.directories.contains(&"/global/dir1".to_string()));
        assert!(loaded.directories.contains(&"/global/dir2".to_string()));
        assert!(loaded.directories.contains(&"/project/dir1".to_string()));
    }

    #[test]
    fn test_directories_only_global() {
        let home = create_home_dir();
        write_global_settings(
            home.path(),
            r#"
directories = ["/global/dir"]

[[models]]
name = "zen"
model = "glm-5.1"
base_url = "https://example.com"
api_key_env = "KEY"
"#,
        );

        let loaded = with_var("HOME", Some(home.path()), || SettingsLoader::load(None)).unwrap();

        assert_eq!(loaded.directories, vec!["/global/dir".to_string()]);
    }

    #[test]
    fn test_directories_empty_by_default() {
        let dir = create_project_settings(
            r#"
[[models]]
name = "zen"
model = "glm-5.1"
base_url = "https://example.com"
api_key_env = "KEY"
"#,
        );

        let home = create_home_dir();
        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(dir.path()))
        })
        .unwrap();

        assert!(loaded.directories.is_empty());
    }

    #[test]
    fn test_project_explicitly_clears_default_model() {
        let home = create_home_dir();
        write_global_settings(
            home.path(),
            r#"
default_model = "global-model"

[[models]]
name = "global-model"
model = "global/model"
base_url = "https://global.example.com"
api_key_env = "GLOBAL_KEY"
"#,
        );

        // Project file has no default_model line at all — global should persist.
        let project_dir = create_project_settings(
            r#"
[[models]]
name = "project-model"
model = "project/model"
base_url = "https://project.example.com"
api_key_env = "PROJECT_KEY"
"#,
        );

        let loaded = with_var("HOME", Some(home.path()), || {
            SettingsLoader::load(Some(project_dir.path()))
        })
        .unwrap();

        // Project didn't set default_model, so global persists
        assert_eq!(loaded.default_model, Some("global-model".to_string()));
    }
}
