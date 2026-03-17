# CLI Module

The CLI module provides the command-line interface for acai, handling argument parsing and user-facing error messages.

## Overview

The CLI layer is intentionally thin—it delegates all business logic to lower layers while handling:

- **Argument parsing**: Using `clap` to define and validate command-line flags
- **User interaction**: Reading from stdin, handling worktrees, and formatting output
- **Session lifecycle**: Managing session creation, continuation, resumption, and forking

## Architecture

### CodingAssistant Struct

The main CLI is implemented as a single struct using `clap`'s derive macro:

```rust
#[derive(Parser, Debug, Clone)]
#[command(name = "acai")]
#[command(about = "AI coding assistant CLI")]
pub struct CodingAssistant {
    /// The prompt to send to the AI (use `-` to read from stdin)
    pub prompt: Option<String>,
    
    /// Sets the model to use (e.g., "minimax/minimax-m2.5")
    #[arg(long)]
    pub model: Option<String>,
    
    /// Sets the temperature value
    #[arg(long)]
    pub temperature: Option<f32>,
    
    /// Sets the max tokens value
    #[arg(long)]
    pub max_tokens: Option<u32>,
    
    /// Sets the top-p value
    #[arg(long)]
    pub top_p: Option<f32>,
    
    /// Output format (text or stream-json)
    #[arg(long)]
    pub output_format: Option<OutputFormat>,
    
    /// Continue the most recent session
    #[arg(long)]
    pub continue_session: bool,
    
    /// Resume a specific session by UUID
    #[arg(long)]
    pub resume: Option<String>,
    
    /// Fork a session into a new one
    #[arg(long)]
    pub fork: Option<String>,
    
    /// Do not save the session
    #[arg(long)]
    pub no_session: bool,
    
    /// Run in an isolated git worktree
    #[arg(short, long)]
    pub worktree: Option<Option<String>>,
    
    /// Restrict which providers can serve requests
    #[arg(long)]
    pub providers: Option<Vec<String>>,
}
```

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
acai "Implement a binary search tree"

# Read from stdin
cat file.txt | acai "Summarize this"

# Heredoc
acai << 'EOF'
Implement a function that:
1. Takes a list of numbers
2. Returns the sum
EOF

# Explicit stdin with dash
echo "Hello" | acai -
```

## Output Formats

Two output formats are supported:

- **`text`** (default): Human-readable text output
- **`stream-json`**: Machine-readable JSON streaming with events for each conversation item

When using `stream-json`, console logging is automatically suppressed to avoid polluting stdout.
