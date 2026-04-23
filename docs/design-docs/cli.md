# CLI Module

The CLI module provides the command-line interface for cake, handling argument parsing and user-facing error messages.

## Overview

The CLI layer is intentionally thin—it delegates all business logic to lower layers while handling:

- **Argument parsing**: Using `clap` to define and validate command-line flags
- **User interaction**: Reading from stdin, handling worktrees, and formatting output
- **Session lifecycle**: Managing session creation, continuation, resumption, and forking

## Architecture

### CodingAssistant Struct

The main CLI is implemented as a single struct using `clap`'s derive macro:

```rust
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct CodingAssistant {
    /// The prompt to send to the AI (use `-` to read from stdin)
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,

    /// Sets the max tokens value
    #[arg(long)]
    pub max_tokens: Option<u32>,

    /// Output format for the response (text or stream-json)
    #[arg(long, value_enum, default_value = "text")]
    pub output_format: OutputFormat,

    /// Continue the most recent session for this directory
    #[arg(long = "continue")]
    pub continue_session: bool,

    /// Resume a specific session by UUID
    #[arg(long, value_name = "UUID")]
    pub resume: Option<String>,

    /// Fork a session: copy its history into a new session with a fresh ID.
    /// Use without a value to fork the latest session, or provide a UUID.
    #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "UUID")]
    pub fork: Option<String>,

    /// Do not save the session to disk
    #[arg(long)]
    pub no_session: bool,

    /// Run in an isolated git worktree (optionally provide a name)
    #[arg(short, long, num_args = 0..=1, default_missing_value = "", value_name = "NAME")]
    pub worktree: Option<String>,

    /// Select a model by name from settings.toml
    #[arg(long)]
    pub model: Option<String>,

    /// Override reasoning effort level (none, low, medium, high, xhigh)
    #[arg(long, value_name = "EFFORT")]
    pub reasoning_effort: Option<String>,

    /// Override reasoning token budget
    #[arg(long, value_name = "TOKENS")]
    pub reasoning_budget: Option<u32>,

    /// Add a directory to the sandbox config (read-only access). Can be repeated.
    #[arg(long, value_name = "DIR")]
    pub add_dir: Vec<String>,
}
```

### Model Configuration

Model-related settings (`model`, `temperature`, `top_p`, `api_type`, etc.) are configured via:

1. **Settings TOML** (`settings.toml`): Named models can be defined in settings files (see below)
2. **`ModelConfig` in `config::model`**: Default values for the built-in model
3. **CLI flags**: `--model` to select a named model, `--max-tokens` for token override

#### Settings TOML

cake supports loading model configurations from `settings.toml` files:

- **Project-level**: `.cake/settings.toml` in the current working directory
- **Global**: `~/.config/cake/settings.toml` for system-wide settings

Settings are merged with project settings overriding global settings for models with the same name. This allows you to define base configurations globally and override specific models per-project.

```toml
[[models]]
name = "zen"                    # Required: unique name (lowercase alphanumeric + hyphens)
model = "glm-5"                 # Required: model identifier
base_url = "https://opencode.ai/zen/go/v1/"  # Optional
api_key_env = "OPENCODE_ZEN_API_TOKEN"        # Optional
api_type = "chat_completions"   # Optional: chat_completions or responses
temperature = 0.8               # Optional
top_p = 0.9                    # Optional: nucleus sampling (alternative to temperature)
max_output_tokens = 8000      # Optional
providers = []                 # Optional

[[models]]
name = "claude"
model = "anthropic/claude-3-sonnet"
base_url = "https://openrouter.ai/api/v1/"
api_key_env = "OPENROUTER_API_KEY"
api_type = "responses"
temperature = 0.7
top_p = 0.9
```

Use `--model <name>` to select a named model from settings:

```bash
# Use the "claude" model from settings.toml
cake --model claude "Your prompt here"
```

If `--model` is not provided, cake uses the default `ModelConfig` (currently GLM-5 via OpenCode).

The struct implements the `CmdRunner` trait for execution:

```rust
impl CmdRunner for CodingAssistant {
    async fn run(&self, data_dir: &DataDir) -> anyhow::Result<()> {
        // Validate flags, build client, run the conversation
    }
}
```

## Session Management

The CLI handles four session modes:

1. **New Session** (default): Creates a fresh session with a new UUID
2. **Continue** (`--continue`): Loads the most recent session for the current directory
3. **Resume** (`--resume <UUID>`): Loads a specific session by UUID
4. **Fork** (`--fork [UUID]`): Copies history from an existing session into a new session

These modes are mutually exclusive—only one can be used at a time.

## Input Sources

The CLI accepts input from multiple sources:

1. **`[PROMPT]`**: Positional argument for the prompt (use `-` to read from stdin)
2. **stdin**: Pipe input or use heredocs for multi-line prompts

The prompt and stdin can be combined—the prompt is used as instructions with stdin content appended.

### Examples

```bash
# Positional prompt
cake "Implement a binary search tree"

# Read from stdin
cat file.txt | cake "Summarize this"

# Heredoc
cake << 'EOF'
Implement a function that:
1. Takes a list of numbers
2. Returns the sum
EOF

# Explicit stdin with dash
echo "Hello" | cake -
```

## Related Documentation

- [prompts.md](./prompts.md): System prompt construction and AGENTS.md integration
- [session-management.md](./session-management.md): Session lifecycle and storage

## Output Formats

Two output formats are supported:

- **`text`** (default): Human-readable text output. Progress is streamed to stderr while the final assistant message is printed to stdout. The final progress line includes the session ID along with duration, turn count, and token usage.
- **`stream-json`**: Machine-readable JSON streaming with events for each conversation item

When using `stream-json`, console logging is automatically suppressed to avoid polluting stdout.

## Exit Codes

cake returns structured exit codes so that shell scripts and CI pipelines can branch on the reason for failure:

| Code | Name         | Description                                              |
|------|--------------|----------------------------------------------------------|
| `0`  | Success      | The agent completed and produced a response               |
| `1`  | Agent error  | The model or a tool encountered an error during execution|
| `2`  | API error    | Rate limit, auth failure, or network error               |
| `3`  | Input error  | No prompt provided, invalid flags, missing API key       |

### Classification Logic

The `exit_code` module classifies `anyhow::Error` values by inspecting the error chain:

1. **Input errors** (exit 3): Matched by message patterns such as "Environment variable ... is not set", "No input provided", "Invalid model name", "Unknown model", "Invalid session UUID", and clap argument errors.
2. **API errors** (exit 2): Matched by `reqwest::Error` downcast (401/403, connect, timeout, request errors) or message patterns containing "429", "401", "403", "rate_limit", "authentication", "connection refused", etc.
3. **Agent/tool errors** (exit 1): The default for any error not matching the above categories.

The `main()` function returns `std::process::ExitCode` directly (not `anyhow::Result`), classifying errors before exiting. This replaces the previous behavior where all errors produced exit code 1.

### Streaming JSON Integration

When using `--output-format stream-json`, the result event includes an `exit_code` field:

```json
{"type":"result","success":true,"exit_code":0,...}
{"type":"result","success":false,"exit_code":2,"error":"...",...}
```
