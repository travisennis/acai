# CLI Module

The CLI module provides the command-line interface for acai, handling argument parsing, command dispatch, and user-facing error messages.

## Overview

The CLI layer is intentionally thin—it delegates all business logic to lower layers while handling:

- **Argument parsing**: Using `clap` to define and validate command-line flags
- **Command dispatch**: Routing to the appropriate command implementation via the `CmdRunner` trait
- **User interaction**: Reading from stdin, handling worktrees, and formatting output
- **Session lifecycle**: Managing session creation, continuation, resumption, and forking

## Architecture

### CmdRunner Trait

The `CmdRunner` trait defines the interface that all commands must implement:

```rust
pub trait CmdRunner {
    async fn run(&self, data_dir: &DataDir) -> anyhow::Result<()>;
}
```

This trait-based design allows for:
- Easy addition of new commands
- Consistent error handling across commands
- Dependency injection for testing

### Commands

Currently implemented commands:

- **`instruct`**: The primary command for sending instructions to the AI

### Instruct Command

The `instruct` command (`src/cli/cmds/instruct.rs`) is the main interface:

```rust
pub struct Cmd {
    /// Sets the model to use (e.g., "minimax/minimax-m2.5")
    pub model: String,
    /// Sets the temperature value
    pub temperature: Option<f32>,
    /// Sets the max tokens value
    pub max_tokens: Option<u32>,
    /// Sets the top-p value
    pub top_p: Option<f32>,
    /// Sets the prompt
    pub prompt: Option<String>,
    /// Read the prompt from a file
    pub prompt_file: Option<String>,
    /// Output format (text or stream-json)
    pub output_format: OutputFormat,
    /// Continue the most recent session
    pub continue_session: bool,
    /// Resume a specific session by UUID
    pub resume: Option<String>,
    /// Fork a session into a new one
    pub fork: Option<String>,
    /// Do not save the session
    pub no_session: bool,
    /// Run in an isolated git worktree
    pub worktree: Option<String>,
    /// Restrict which providers can serve requests
    pub providers: Vec<String>,
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

The CLI accepts input from multiple sources (in order of precedence):

1. **`--prompt "text"`**: Direct command-line prompt
2. **`--prompt-file path`**: Read prompt from a file
3. **stdin**: Pipe input (only if stdin is not a TTY)

If multiple sources are provided, they are combined with the prompt first, then stdin context.

## Output Formats

Two output formats are supported:

- **`text`** (default): Human-readable text output
- **`stream-json`**: Machine-readable JSON streaming with events for each conversation item

When using `stream-json`, console logging is automatically suppressed to avoid polluting stdout.

## Git Worktree Support

The `--worktree` flag enables isolated execution environments:

1. Creates a new git worktree (or uses an existing one)
2. Changes the working directory to the worktree
3. After execution, removes the worktree if there are no changes, or keeps it if there are

This allows safe experimentation without affecting the main working tree.

## Error Handling

The CLI layer handles user-facing errors:

- **Validation errors**: Mutually exclusive flags, invalid UUIDs
- **Input errors**: Missing prompts, file not found
- **API errors**: Network issues, rate limits
- **Tool errors**: Sandboxing violations, path validation failures

Errors are reported to stderr, and the process exits with a non-zero status.

## Integration with Lower Layers

The CLI integrates with:

- **`clients::responses`**: Creates and configures the API client
- **`clients::types`**: Handles conversation items for streaming output
- **`config`**: Loads/saves sessions, reads AGENTS.md files
- **`prompts`**: Builds the system prompt with context
- **`models`**: Constructs user messages

## Testing

The CLI module includes tests for:

- Flag validation and mutual exclusivity
- Input source combination
- Session lifecycle (new, continue, resume, fork)
- Worktree setup and cleanup

Tests use temporary directories and mock inputs to avoid side effects.
