# Settings TOML

This document describes the settings system that allows configuration of models and other settings via TOML files.

## Overview

cake supports loading configuration from `settings.toml` files, enabling:

- **Named model configurations**: Define multiple models with different settings
- **Project-level settings**: Per-project `.cake/settings.toml`
- **Global settings**: System-wide `~/.cache/cake/settings.toml`
- **Merge semantics**: Project settings override global settings for conflicting model names

## File Locations

Settings files are loaded from two locations:

| Location | Purpose |
|----------|---------|
| `~/.cache/cake/settings.toml` | Global/system-wide settings |
| `./.cake/settings.toml` | Project-specific settings |

Both files are optional. If neither exists, cake uses the default `ModelConfig`.

## Merge Behavior

Settings are merged with the following rules:

1. **Global settings loaded first**: `~/.cache/cake/settings.toml` is loaded into a map
2. **Project settings overlay**: `./.cake/settings.toml` is loaded and added to the map
3. **Project overrides global**: If the same model name exists in both, project wins
4. **No in-file duplicates**: A single file cannot define the same model name twice (error)

This allows you to:
- Define base models globally
- Override specific models per-project
- Add project-specific models without affecting global config

## TOML Format

```toml
[[models]]
# Required: unique identifier for this model (lowercase alphanumeric + hyphens)
name = "zen"

# Required: model identifier (e.g., "glm-5", "anthropic/claude-3-sonnet")
model = "glm-5"

# Optional: API endpoint base URL (defaults to OpenCode default)
base_url = "https://opencode.ai/zen/go/v1/"

# Optional: environment variable name for API key (defaults to OpenCode default)
api_key_env = "OPENCODE_ZEN_API_TOKEN"

# Optional: API type - "chat_completions" or "responses" (defaults to chat_completions)
api_type = "chat_completions"

# Optional: sampling temperature (no default if omitted)
temperature = 0.8

# Optional: nucleus sampling parameter (alternative to temperature, no default if omitted)
top_p = 0.9

# Optional: maximum output tokens (no default if omitted)
max_output_tokens = 8000

# Optional: reasoning effort level (none, low, medium, high, xhigh)
reasoning_effort = "high"

# Optional: reasoning summary mode (Responses API only)
reasoning_summary = "concise"

# Optional: maximum reasoning tokens budget
reasoning_max_tokens = 10000

# Optional: provider routing hints (defaults to empty array)
providers = []

[[models]]
name = "claude"
model = "anthropic/claude-3-sonnet"
base_url = "https://openrouter.ai/api/v1/"
api_key_env = "OPENROUTER_API_KEY"
api_type = "responses"
temperature = 0.7
top_p = 0.9
```

## Required vs Optional Fields

### Required Fields

| Field | Description |
|-------|-------------|
| `name` | Unique identifier for the model. Must be lowercase alphanumeric with hyphens only (`^[a-z0-9-]+$`) |
| `model` | Model identifier string |

### Optional Fields

All other fields have defaults matching `ModelConfig::default()`:

| Field | Default | Description |
|-------|---------|-------------|
| `base_url` | OpenCode default | API endpoint base URL |
| `api_key_env` | OpenCode default | Environment variable name for API key |
| `api_type` | `chat_completions` | API format (`chat_completions` or `responses`) |
| `temperature` | `None` | Sampling temperature |
| `top_p` | `None` | Nucleus sampling parameter |
| `max_output_tokens` | `None` | Maximum output tokens |
| `providers` | `[]` | Provider routing hints |
| `reasoning_effort` | `None` | Reasoning effort level (none, low, medium, high, xhigh) |
| `reasoning_summary` | `None` | Reasoning summary mode (Responses API only) |
| `reasoning_max_tokens` | `None` | Maximum reasoning tokens budget |

## Model Name Validation

Model names must:

1. **Be non-empty**: Empty strings are rejected
2. **Use only lowercase**: Uppercase letters are not allowed
3. **Use only alphanumeric + hyphens**: Spaces, underscores, dots, etc. are not allowed
4. **Be unique per file**: Duplicate names within a single file cause an error

Valid examples: `zen`, `deepseek-chat`, `model-123`

Invalid examples: `My Model`, `deepseek_chat`, `model.123`, ``

## CLI Integration

The `--model` flag selects a named model from settings:

```bash
# Select "claude" model from settings.toml
cake --model claude "Your prompt here"

# Select "deepseek" model
cake --model deepseek "Your prompt here"
```

### Behavior

| Flag | Settings Found | Behavior |
|------|----------------|----------|
| `--model foo` | Yes | Use model config from settings |
| `--model foo` | No | Error with available model names |
| No `--model` | N/A | Use `ModelConfig::default()` |

### Error Messages

Invalid model names produce helpful errors:

```
Invalid model name 'My Model': name cannot contain uppercase letters, spaces, or special characters.
Model names must contain only lowercase letters, numbers, and hyphens.
```

Unknown models list available options:

```
Unknown model 'nonexistent'. Available models: zen, claude, deepseek.
- Use a model name from settings.toml, or omit --model to use the default.
```

## Implementation

### Key Types

```rust
// Settings file structure
struct Settings {
    models: Vec<ModelDefinition>,
}

// Individual model definition
struct ModelDefinition {
    name: String,              // Required
    model: String,             // Required
    base_url: String,          // Optional, defaults in code
    api_key_env: String,       // Optional, defaults in code
    api_type: ApiType,         // Optional, defaults to ChatCompletions
    temperature: Option<f32>,  // Optional
    top_p: Option<f32>,         // Optional
    max_output_tokens: Option<u32>,  // Optional
    reasoning_effort: Option<String>,  // Optional
    reasoning_summary: Option<String>,  // Optional
    reasoning_max_tokens: Option<u32>,  // Optional
    providers: Vec<String>,     // Optional, defaults to []
}
```

### SettingsLoader

The `SettingsLoader` handles loading and merging:

```rust
impl SettingsLoader {
    /// Load and merge settings from global and project locations.
    pub fn load(
        project_dir: Option<&Path>,
        global_dir: &Path,
    ) -> Result<HashMap<String, ModelDefinition>, SettingsError>;
}
```

## Example Workflow

### 1. Create global settings

`~/.cache/cake/settings.toml`:
```toml
[[models]]
name = "deepseek"
model = "deepseek/deepseek-chat-v3"
base_url = "https://openrouter.ai/api/v1/"
api_key_env = "OPENROUTER_API_KEY"
```

### 2. Create project settings

`.cake/settings.toml`:
```toml
[[models]]
name = "claude"
model = "anthropic/claude-3-sonnet"
base_url = "https://openrouter.ai/api/v1/"
api_key_env = "OPENROUTER_API_KEY"
api_type = "responses"
```

### 3. Use models

```bash
# Uses "claude" from project settings
cake --model claude "Use claude"

# Uses "deepseek" from global settings
cake --model deepseek "Use deepseek"

# Uses default (no settings needed)
cake "Use default model"
```

## Future Considerations

Potential extensions:

- **Additional settings sections**: Beyond `models`, other configuration could be added
- **Validation hooks**: Custom validation for model configurations
- **Secret management**: Support for fetching API keys from secret managers
- **Model aliases**: Shorthand names that resolve to full configurations
