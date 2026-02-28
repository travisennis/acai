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

### Options

- `--model` - Set the model to use (default: `minimax/minimax-m2.5`)
- `--temperature` - Set the temperature (0.0 to 1.0)
- `--max-tokens` - Set maximum tokens in response
- `--top-p` - Set top-p value
- `--prompt` (`-p`) - Your instruction prompt

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
