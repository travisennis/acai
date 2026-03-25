# acai

acai is a minimal coding harness for headless usage in the terminal.

## Features

- Send instructions to AI for code generation or documentation
- Supports multiple AI providers via configurable API endpoints
- Default model: GLM-5 via OpenCode
- OS-level filesystem sandbox for Bash tool commands (macOS sandbox-exec, Linux Landlock)
- Conversation session management with continue, resume, and fork capabilities
- Git worktree integration for isolated development environments

## Installation

To install acai, you'll need Rust and Cargo installed on your system. Then, follow these steps:

1. Clone the repository:
   ```bash
   git clone https://github.com/travisennis/acai.git
   cd acai
   ```

2. Build the project:
   ```bash
   cargo build --release
   ```

3. The binary will be available in `target/release/acai`

## Usage

```bash
# Basic usage with a prompt
acai "Implement a binary search tree in Rust"

# Pipe file content with instructions
cat src/main.rs | acai "Explain this code"

# Use a heredoc for multi-line prompts
acai << 'EOF'
Implement a function that:
1. Takes a list of numbers
2. Returns the sum
EOF

# Heredoc with prompt prefix
acai "Review this code:" << 'EOF'
fn main() {
    println!("Hello");
}
EOF

# Input redirection
acai < prompt.txt

# Read from stdin explicitly
acai - < file.txt

# With max tokens override
acai --max-tokens 4000 "Your prompt here"
```

## Configuration

acai requires an API key for the AI provider. Set your API key as an environment variable:

- `OPENCODE_ZEN_API_TOKEN`: Your OpenCode Zen API key (default)

Or configure a different provider by setting the appropriate environment variable for your chosen endpoint.

### Model Configuration

Model settings can be configured via:

1. **Settings TOML**: Define named models in `settings.toml` files
2. **Environment variables**: Set your API key (e.g., `OPENCODE_ZEN_API_TOKEN`)
3. **CLI flags**: `--model` to select a named model, `--max-tokens` to override

#### Settings TOML

Create a `settings.toml` file to define custom model configurations:

- **Project-level**: `.acai/settings.toml` in your project directory
- **Global**: `~/.cache/acai/settings.toml` for system-wide settings

```toml
# Example settings.toml
[[models]]
name = "claude"                    # Use with --model claude
model = "anthropic/claude-3-sonnet"
base_url = "https://openrouter.ai/api/v1/"
api_key_env = "OPENROUTER_API_KEY"
api_type = "responses"
temperature = 0.7

[[models]]
name = "deepseek"
model = "deepseek/deepseek-chat-v3"
base_url = "https://openrouter.ai/api/v1/"
api_key_env = "OPENROUTER_API_KEY"
top_p = 0.9
```

```bash
# Use a named model from settings.toml
acai --model claude "Your prompt here"

# Without --model, uses default (GLM-5 via OpenCode)
acai "Your prompt here"
```

See `.acai/settings.toml.example` for a complete example.

#### Default Configuration

When not using settings.toml, acai uses these defaults:

- **Model**: `glm-5`
- **API Endpoint**: `https://opencode.ai/zen/go/v1`
- **API Key Env**: `OPENCODE_ZEN_API_TOKEN`
- **Temperature**: 0.8
- **Max Output Tokens**: 8000

### Session Management

acai automatically saves conversation sessions so you can continue conversations across separate invocations. Sessions are tracked per directory.

```bash
# Start a conversation
acai "Remember the number 42"

# Continue the most recent session in the current directory
acai --continue "What number did I tell you?"

# Resume a specific session by UUID
acai --resume 550e8400-e29b-41d4-a716-446655440000 "Continue our conversation"

# Fork the latest session (creates new session with same history)
acai --fork "Let's discuss something different"

# Fork a specific session by UUID
acai --fork 550e8400-e29b-41d4-a716-446655440000 "New branch of conversation"
```

Sessions are saved to `~/.cache/acai/sessions/` and include full conversation history with metadata. Sessions are saved on both success and error for crash recovery.

For more details, see [Session Management](docs/design-docs/session-management.md).

### Worktrees

Run a task in an isolated git worktree so changes don't affect your main working directory. The worktree is created at `<repo>/.acai/worktrees/<name>` on a new branch based on the default remote branch.

```bash
# Named worktree
acai -w feature-auth "Add auth middleware"

# Auto-generated name
acai -w "Fix the bug"
```

When the task finishes, acai automatically removes the worktree if no changes were made. If there are uncommitted changes or new commits, the worktree is kept so you can return to it later.

### Filesystem Sandbox

Commands executed by the Bash tool run inside an OS-level filesystem sandbox that restricts access to only the project directory and essential system paths. This prevents LLM-generated commands from reading or writing files outside the allowed set.

- **macOS**: Uses `sandbox-exec` with a deny-default Seatbelt profile
- **Linux**: Uses Landlock LSM (kernel 5.13+, requires `--features landlock`)

The sandbox can be disabled by setting `ACAI_SANDBOX=off`.

For more details, see [Filesystem Sandbox](docs/design-docs/sandbox.md).

### Options

- `[PROMPT]` - Your instruction prompt as a positional argument (use `-` to read from stdin)
- `--max-tokens` - Set maximum tokens in response
- `--output-format` - Output format: `text` (default) or `stream-json`
- `--model <NAME>` - Select a named model from settings.toml
- `--continue` - Continue the most recent session for the current directory
- `--resume <UUID>` - Resume a specific session by its UUID
- `--fork [UUID]` - Fork a session (copy history into new session), optionally specify UUID
- `--no-session` - Do not save the session to disk
- `--worktree` (`-w`) - Run in an isolated git worktree (optionally provide a name)

### Example

```bash
export OPENCODE_ZEN_API_TOKEN=your_api_key_here
acai --max-tokens 4000 "Explain what this code does"
```

## Architecture

acai follows a layered architecture with strict dependency flow:

1. **CLI Layer**: Argument parsing and user interaction
2. **Clients Layer**: AI service integration, tool execution, and conversation orchestration
3. **Config/Models/Prompts Layer**: Data persistence, core types, and prompt generation

For detailed architecture documentation, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Contributing

Contributions to acai are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, build commands, code style guidelines, commit conventions, and the pull request process.

## Testing

To run the test suite:

```bash
cargo test
```

## License

acai is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Acknowledgements

acai uses several open-source libraries and AI models. We're grateful to the developers and organizations behind these technologies:

- Rust and the Rust community for providing excellent tools and libraries that make projects like this possible.
- OpenCode and OpenRouter for AI model access.
- The developers of crates used in this project (tokio, clap, reqwest, and others). Please see the `Cargo.toml` file for a full list of dependencies.
