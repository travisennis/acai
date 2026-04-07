# acai

acai is a minimal coding harness for headless usage in the terminal. It's not a TUI — it's a Unix filter for AI. It takes input, does work, produces output, and exits. That's its strength: acai is composable with every tool in your shell.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
  - [Shell Pipelines](#shell-pipelines)
  - [Multi-file Context](#multi-file-context)
- [Configuration](#configuration)
  - [Model Configuration](#model-configuration)
  - [Reasoning Configuration](#reasoning-configuration)
  - [Default Configuration](#default-configuration)
- [Session Management](#session-management)
  - [Branching Conversations](#branching-conversations)
- [Worktrees](#worktrees)
- [Filesystem Sandbox](#filesystem-sandbox)
  - [Destructive Command Protection](#destructive-command-protection)
  - [Adding Read-Only Directories](#adding-read-only-directories)
- [AGENTS.md — Per-Project AI Behavior](#agentsmd--per-project-ai-behavior)
- [Shell Aliases and Functions](#shell-aliases-and-functions)
- [Streaming JSON Output](#streaming-json-output)
- [Options](#options)
- [Architecture](#architecture)
- [Contributing](#contributing)
- [Testing](#testing)
- [License](#license)
- [Acknowledgements](#acknowledgements)

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

### Shell Pipelines

acai reads from stdin, so it composes naturally with other Unix tools:

```bash
# Code review from git diff
git diff HEAD~3 | acai "Summarize these changes for a changelog entry"

# Explain a file
cat src/main.rs | acai "Explain this code"

# Review staged changes
git diff --staged | acai "Code review these staged changes"
```

### Multi-file Context

Use heredocs with command substitution to feed multiple files as context:

```bash
acai << 'EOF'
Here are two files. Explain how they interact:
--- agent.rs ---
$(cat src/clients/agent.rs)
--- types.rs ---
$(cat src/clients/types.rs)
EOF
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

[[models]]
name = "o4-mini"
model = "openai/o4-mini"
api_type = "responses"
reasoning_effort = "high"          # none|low|medium|high|xhigh
reasoning_summary = "concise"      # concise|detailed|auto (Responses API only)

[[models]]
name = "claude-reasoning"
model = "anthropic/claude-3.7-sonnet"
base_url = "https://openrouter.ai/api/v1/"
api_key_env = "OPENROUTER_API_KEY"
api_type = "responses"
reasoning_max_tokens = 8000        # Budget-style for Anthropic via OpenRouter
```

```bash
# Use a named model from settings.toml
acai --model claude "Your prompt here"

# Without --model, uses default (GLM-5 via OpenCode)
acai "Your prompt here"
```

See `.acai/settings.toml` for a complete example.

#### Reasoning Configuration

Models that support reasoning (e.g., OpenAI o-series, Anthropic Claude with extended thinking) can be configured with these fields:

| Field | Description | Values |
|-------|-------------|--------|
| `reasoning_effort` | Controls how much reasoning the model performs | `none`, `low`, `medium`, `high`, `xhigh` |
| `reasoning_summary` | How reasoning is summarized (Responses API only) | `concise`, `detailed`, `auto` |
| `reasoning_max_tokens` | Token budget for reasoning (budget-style) | Any positive integer |

These can also be overridden at runtime with CLI flags:

```bash
# Override reasoning effort for a single run
acai --reasoning-effort high "Solve this math problem"

# Set a reasoning token budget
acai --reasoning-budget 4000 "Analyze this code"

# Combine with a named model
acai --model claude --reasoning-effort medium "Explain this algorithm"
```

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

#### Branching Conversations

Think of session management as branches of thought:

- **`--continue`** is your "keep going" — great for multi-step tasks where acai needs to iterate.
- **`--fork`** is your "what if?" — try a different approach without losing the original thread.
- **`--resume <UUID>`** is your "go back to that idea from Tuesday."
- **`--no-session`** is for throwaway questions that don't pollute your session history.

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

#### Destructive Command Protection

The Bash tool also blocks known-destructive commands before execution, covering destructive git operations (`git reset --hard`, `git push --force`, `git clean -f`, etc.) and `rm -rf` outside temp directories. This includes detection of commands wrapped in `bash -c` or `sh -c`. A blocked command returns an error explaining the reason and suggesting a safe alternative.

#### Adding Read-Only Directories

Use `--add-dir` to grant the agent read-only access to directories outside the project:

```bash
# Allow the agent to read from a shared library directory
acai --add-dir /path/to/shared/libs "Use the utilities in /path/to/shared/libs"

# Multiple directories can be added
acai --add-dir ~/Documents/references --add-dir ~/Projects/shared "Review the code"
```

The agent will be able to **read** files from these directories but **not write** to them.

For more details, see [Filesystem Sandbox](docs/design-docs/sandbox.md).

### AGENTS.md — Per-Project AI Behavior

acai reads `AGENTS.md` files to shape its behavior without re-prompting every time:

- **`~/.acai/AGENTS.md`** — Global personality, preferences, and conventions applied to all projects.
- **`./AGENTS.md`** — Project-level instructions: tech stack, coding standards, domain knowledge.

This is how you make acai a domain expert. For example, a project-level `AGENTS.md` might say:

```markdown
This is a Rust project using Tokio for async. Use `anyhow` for errors.
Always run `cargo fmt` and `cargo clippy` after editing Rust files.
Never use `unwrap()` in production code.
```

### Shell Aliases and Functions

Set up shell aliases to turn common patterns into one-liners:

```bash
# Quick aliases
alias review='git diff --staged | acai "Code review these staged changes"'
alias explain='acai "Explain this code:" < '
alias changelog='git log --oneline HEAD~10..HEAD | acai "Write a changelog from these commits"'

# Multi-model comparison
compare() { acai --no-session --model glm "$1" & acai --no-session --model qwen "$1" & wait; }
```

### Streaming JSON Output

The `--output-format stream-json` mode emits NDJSON events for every conversation item, turning acai into a **backend for any frontend**. You can build a tmux-pane viewer, a web UI, or a VS Code extension that consumes the stream.

```bash
acai --output-format stream-json "List files" | jq '.type'
```

See [Streaming JSON Output](docs/design-docs/streaming-json-output.md) for the full schema.

### Options

- `[PROMPT]` - Your instruction prompt as a positional argument (use `-` to read from stdin)
- `--max-tokens` - Set maximum tokens in response
- `--output-format` - Output format: `text` (default) or `stream-json`
- `--model <NAME>` - Select a named model from settings.toml
- `--continue` - Continue the most recent session for the current directory
- `--resume <UUID>` - Resume a specific session by its UUID
- `--fork [UUID]` - Fork a session (copy history into new session), optionally specify UUID
- `--verbose` - Show tool call progress on stderr (only with `text` output format)
- `--no-session` - Do not save the session to disk
- `--worktree` (`-w`) - Run in an isolated git worktree (optionally provide a name)
- `--reasoning-effort <EFFORT>` - Override reasoning effort level (none, low, medium, high, xhigh)
- `--reasoning-budget <TOKENS>` - Override reasoning token budget
- `--add-dir <DIR>` - Add a directory to the sandbox config (read-only access). Can be repeated.

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
