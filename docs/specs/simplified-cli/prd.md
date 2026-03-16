# Simplified CLI Interface - Product Requirements Document

## Problem Statement

The current CLI interface requires verbose invocation patterns that add friction to the user experience. Users must type `acai instruct --prompt "..."` or `acai instruct -p "..."` for every interaction, despite `instruct` being the only command and the prompt being the primary input. This verbosity is unnecessary and slows down the workflow, especially when used frequently throughout the day.

## Solution

Transform the CLI to accept prompts as positional arguments at the root level, eliminating the need for the `instruct` subcommand and `--prompt` flag. The new interface will support:

- Direct prompt execution: `acai "your prompt here"`
- Stdin-based input: `cat file.txt | acai "summarize this"`
- Pure stdin mode: `acai -` or `cat prompt.txt | acai`
- Full codex-style piping and heredoc support

This change makes the tool feel more like a natural extension of the shell while preserving the ability to add future subcommands.

## User Stories

1. As a developer, I want to run `acai "refactor this function"` without typing `instruct --prompt`, so that I can iterate faster.

2. As a developer, I want to pipe file content to acai with instructions, so that I can process files without manual copy-pasting.

3. As a developer, I want to use heredocs for multi-line prompts, so that I can write complex instructions naturally in shell scripts.

4. As a developer, I want `acai -` to read the entire prompt from stdin, so that I can chain it with other shell commands.

5. As a developer, I want to redirect a file as the prompt, so that `acai < prompt.txt` works as expected.

6. As a developer, I want to combine piped input with a positional prompt, so that `cat code.py | acai "review this"` sends both the code and instructions.

7. As a developer, I want all existing flags (--model, --temperature, --worktree, etc.) to continue working with the new syntax, so that I don't lose functionality.

8. As a developer, I want the old `instruct` command to be removed (not deprecated), so that there's only one way to use the tool.

9. As a developer, I want to retain the ability to add future subcommands (like `acai config` or `acai list-sessions`), so that the CLI can grow without breaking changes.

10. As a developer, I want error messages to clearly indicate when no input is provided, so that I understand how to fix the command.

## Implementation Decisions

- **Command structure**: The root CLI will accept a positional argument for the prompt. Additional subcommands can be added later using clap's subcommand support with a default command fallback.

- **Argument parsing**: Use clap's `Args` derive at the top level instead of subcommands for the default execution path. The prompt argument will use `value_name = "PROMPT"` and accept `-` as a special value meaning "read from stdin."

- **Stdin handling**: When the prompt is `-` or when no prompt is provided and stdin is not a TTY, read the entire prompt from stdin. When a prompt is provided AND stdin has content, concatenate them with the stdin content appended after a newline separator.

- **Flag preservation**: All existing flags (--model, --temperature, --max-tokens, --top-p, --output-format, --continue, --resume, --fork, --no-session, --worktree, --providers) remain unchanged and available at the top level.

- **Remove instruct module**: The `instruct` subcommand and its associated module will be removed entirely. The core logic will be moved to the top-level CLI or a shared module.

- **Prompt file removal**: The `--prompt-file` flag will be removed since stdin redirection (`acai < file.txt`) and shell command substitution provide equivalent functionality.

- **Error messaging**: When no input is detected (no positional arg, no stdin content), display a helpful error: "No input provided. Provide a prompt as an argument, use 'acai -' for stdin, or pipe input to acai."

## Testing Decisions

- **Unit tests**: Add tests for the argument parsing logic covering:
  - Positional prompt provided
  - Prompt is `-` with stdin content
  - No prompt with piped stdin
  - Prompt combined with piped stdin
  - No input at all (error case)

- **Integration tests**: Test the full command execution with various input methods:
  - `acai "simple prompt"`
  - `echo "prompt" | acai`
  - `echo "prompt" | acai -`
  - `acai "instructions" < file.txt`
  - `acai <<EOF ... EOF`

- **Flag compatibility**: Verify all existing flags work with the new syntax and produce the same behavior as before.

- **Prior art**: Follow the existing test patterns in the codebase for command-line parsing and end-to-end execution testing.

## Out of Scope

- Aliases or shortcuts for flags (e.g., `-m` for `--model`) — these can be added separately if desired.
- Interactive mode or REPL — focus is on one-shot execution.
- Changing the default model or any other default behavior.
- Shell completions generation (though the simplified structure may make this easier in the future).

## Further Notes

- The codex CLI serves as the reference implementation for stdin handling behavior. Test cases should match codex's behavior where applicable.

- The `--worktree` flag and its associated functionality should remain unchanged and work seamlessly with the new syntax.

- Session management (--continue, --resume, --fork) continues to work exactly as before.

- The output formats (text and stream-json) are unaffected by this change.

## Linked Issues

- PRD tracking issue: #[to be created]
- Implementation issues:
  - #[to be created] - Refactor CLI structure to use positional prompt argument
  - #[to be created] - Implement stdin handling for `-` and piped input
  - #[to be created] - Remove instruct subcommand and --prompt-file flag
  - #[to be created] - Update documentation and README
