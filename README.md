# acai

Acai is an AI-powered coding assistant that integrates with your development workflow. It provides intelligent code suggestions and documentation generation to enhance your coding experience.

## Features

- Send instructions to AI for code generation or documentation
- Codebase analysis with file filtering
- Support for multiple AI providers (OpenAI, Anthropic, Mistral, Google, Ollama)
- Customizable prompts and templates

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

# With a codebase path for context
acai instruct --path /path/to/your/codebase --prompt "Add unit tests for the auth module"

# With specific file patterns
acai instruct --path /path/to/your/codebase --include "**/*.rs" --exclude "target/**" --prompt "Review this code"

# Using a specific model
acai instruct --model openai/gpt-4o --prompt "Write a hello world program"

# With custom template
acai instruct --template /path/to/template.hbs --prompt "Your prompt here"
```

## Configuration

Acai uses environment variables for API keys:

- `OPENAI_API_KEY`: For OpenAI models
- `ANTHROPIC_API_KEY`: For Anthropic models
- `MISTRAL_API_KEY`: For Mistral models
- `GOOGLE_GENERATIVE_AI_API_KEY`: For Google models

You can also customize the behavior using command-line options:

```bash
acai instruct --help
```

### Options

- `--model` - Set the model to use (e.g., `anthropic/sonnet`, `openai/gpt-4o`)
- `--temperature` - Set the temperature (0.0 to 1.0)
- `--max-tokens` - Set maximum tokens in response
- `--top-p` - Set top-p value
- `--path` - Path to codebase directory
- `--include` - File patterns to include (comma-separated)
- `--exclude` - File patterns to exclude (comma-separated)
- `--template` - Path to Handlebars template
- `--prompt` ( `-p`) - Your instruction prompt

### Example

```bash
acai instruct --model anthropic/sonnet --temperature 0.7 --path ./src --prompt "Explain what this code does"
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
