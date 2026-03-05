# acai

Acai is an AI-powered coding assistant that integrates with your development workflow. It provides intelligent code suggestions and documentation generation to enhance your coding experience.

## Features

- Send instructions to AI for code generation or documentation
- Uses OpenRouter's Responses API for multi-provider access
- Default model: MiniMax MiniMax-M2.5

## Installation

To install Acai, you'll need Rust and Cargo installed on your system. Then, follow these steps:

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

### Instruct Mode

Generate code or documentation based on instructions:

```bash
# Basic usage with a prompt
acai instruct --prompt "Implement a binary search tree in Rust"

# Using a specific model
acai instruct --model anthropic/sonnet-3-5 --prompt "Write a hello world program"

# With temperature
acai instruct --model openai/gpt-4o --temperature 0.7 --prompt "Your prompt here"
```

## Configuration

Acai uses OpenRouter for AI access. Set your API key as an environment variable:

- `OPENROUTER_API_KEY`: Your OpenRouter API key

Get your free API key at [openrouter.ai](https://openrouter.ai).

### Session Management

Acai automatically saves conversation sessions so you can continue conversations across separate invocations. Sessions are tracked per directory.

```bash
# Start a conversation
acai instruct --prompt "Remember the number 42"

# Continue the most recent session in the current directory
acai instruct --continue --prompt "What number did I tell you?"

# Resume a specific session by UUID
acai instruct --resume 550e8400-e29b-41d4-a716-446655440000 --prompt "Continue our conversation"
```

Sessions are saved to `~/.cache/acai/sessions/` and include full conversation history with metadata. Sessions are saved on both success and error for crash recovery.

For more details, see [Session Management](docs/session-management.md).

### Worktrees

Run a task in an isolated git worktree so changes don't affect your main working directory. The worktree is created at `<repo>/.acai/worktrees/<name>` on a new branch based on the default remote branch.

```bash
# Named worktree
acai instruct -w feature-auth -p "Add auth middleware"

# Auto-generated name
acai instruct -w -p "Fix the bug"
```

When the task finishes, acai automatically removes the worktree if no changes were made. If there are uncommitted changes or new commits, the worktree is kept so you can return to it later.

### Options

- `--model` - Set the model to use (default: `minimax/minimax-m2.5`)
- `--temperature` - Set the temperature (0.0 to 1.0)
- `--max-tokens` - Set maximum tokens in response
- `--top-p` - Set top-p value
- `--output-format` - Output format: `text` (default) or `stream-json`
- `--prompt` (`-p`) - Your instruction prompt
- `--continue` - Continue the most recent session for the current directory
- `--resume <UUID>` - Resume a specific session by its UUID
- `--no-session` - Do not save the session to disk
- `--worktree` (`-w`) - Run in an isolated git worktree (optionally provide a name)

### Example

```bash
export OPENROUTER_API_KEY=your_api_key_here
acai instruct --model anthropic/sonnet-3-5 --temperature 0.7 --prompt "Explain what this code does"
```

## Contributing

Contributions to Acai are welcome! Please follow these steps:

1. Fork the repository
2. Create a new branch for your feature or bug fix
3. Make your changes and commit them with clear, descriptive messages
4. Write tests for your new functionality
5. Submit a pull request

Make sure to run tests and linters before submitting:

```bash
cargo test
cargo clippy
```

## Testing

To run the test suite:

```bash
cargo test
```

## License

Acai is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Acknowledgements

Acai uses several open-source libraries and AI models. We're grateful to the developers and organizations behind these technologies:

- Rust and the Rust community for providing excellent tools and libraries that make projects like this possible.
- OpenRouter for providing unified access to multiple AI providers.
- The developers of crates used in this project (tokio, clap, reqwest, and others). Please see the `Cargo.toml` file for a full list of dependencies.
