# acai

Acai is a versatile coding assistant CLI tool that leverages AI models to help developers with various coding tasks, including code completion, optimization, documentation, and more.

## Features

- Interactive chat mode for coding assistance
- Code completion and suggestions
- Code optimization
- Automatic documentation generation
- Bug fixing and code improvement suggestions
- Support for multiple AI providers (OpenAI, Anthropic, Mistral, Google)
- Language Server Protocol (LSP) integration
- Customizable prompts and templates

## Installation

To install Acai, you'll need Rust and Cargo installed on your system. Then, follow these steps:

1. Clone the repository:
   ```
   git clone https://github.com/travisennis/acai.git
   cd acai
   ```

2. Build the project:
   ```
   cargo build --release
   ```

3. The binary will be available in `target/release/acai`

## Usage

Acai provides several subcommands for different functionalities:

- `chat`: Start an interactive chat session with the AI assistant
- `instruct`: Send a single instruction to the AI
- `pipe`: Process input through the AI model
- `complete`: Get code completion suggestions
- `lsp`: Start the Language Server Protocol server

Example usage:

```bash
# Start a chat session
acai chat

# Get code completion
echo "fn main() {" | acai complete

# Optimize code
cat mycode.rs | acai pipe --task optimize

# Generate documentation
cat mycode.rs | acai pipe --task document
```

## Configuration

Acai uses environment variables for API keys:

- `OPENAI_API_KEY`: For OpenAI models
- `CLAUDE_API_KEY`: For Anthropic models
- `MISTRAL_API_KEY`: For Mistral models
- `GOOGLE_API_KEY`: For Google models

You can also specify model and parameters in the command line:

```bash
acai chat --model openai/gpt-4 --temperature 0.7
```

## Contributing

Contributions to Acai are welcome! Please follow these steps:

1. Fork the repository
2. Create a new branch for your feature
3. Implement your changes
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

- Rust and the Rust community
- OpenAI, Anthropic, Mistral, and Google for their AI models
- The developers of crates used in this project (tokio, clap, reqwest, and others)
