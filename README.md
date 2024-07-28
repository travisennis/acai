# acai

Acai is a versatile AI-powered coding assistant that integrates with your development workflow. It provides intelligent code suggestions, documentation generation, and interactive chat capabilities to enhance your coding experience.

## Features

- Interactive chat mode for coding assistance
- Code completion and suggestions
- Code optimization suggestions
- Automatic documentation generation
- Bug fixing and code improvement suggestions
- Support for multiple AI providers (OpenAI, Anthropic, Mistral, Google, Ollama)
- Language Server Protocol (LSP) integration
- Customizable prompts and templates
- File filtering for codebase analysis

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

Acai provides several subcommands for different functionalities:

- `chat`: Start an interactive chat session with the AI assistant
- `instruct`: Send a single instruction to the AI
- `lsp`: Start the Language Server Protocol server

### Chat Mode

Start an interactive chat session with the AI:

```bash
acai chat --model anthropic/claude-3-sonnet-20240229 --path /path/to/your/codebase
```

### Instruct Mode

Generate code or documentation based on instructions:

```bash
acai instruct --model openai/gpt-4 --prompt "Implement a binary search tree in Rust" --path /path/to/your/codebase
```

### Prompt Generator

Generate prompts based on your codebase:

```bash
acai prompt-generator --path /path/to/your/codebase --include "**/*.rs" --exclude "target/**"
```

### LSP Mode

Start Acai as a Language Server:

```bash
acai lsp
```

## Configuration

Acai uses environment variables for API keys:

- `OPENAI_API_KEY`: For OpenAI models
- `CLAUDE_API_KEY`: For Anthropic models
- `MISTRAL_API_KEY`: For Mistral models
- `GOOGLE_API_KEY`: For Google models

You can also customize the behavior using command-line options. Refer to the help command for each subcommand for more details:

```bash
acai <subcommand> --help
```

### Example

```bash
acai chat --model openai/gpt-4 --temperature 0.7
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
- OpenAI, Anthropic, Mistral, and Google for their AI models.
- The developers of crates used in this project (tokio, clap, reqwest, and others). Please see the `Cargo.toml` file for a full list of dependencies.
