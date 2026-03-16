## Parent PRD

[docs/specs/simplified-cli/prd.md](../../prd.md)

## What to build

Refactor the CLI structure to accept a positional `PROMPT` argument at the root level instead of requiring the `instruct` subcommand. This is the foundational change that enables the simplified interface.

Key changes:
- Modify `main.rs` to use `clap::Parser` with a positional argument for the prompt
- Move the core execution logic from `src/cli/cmds/instruct.rs` into the main CLI structure (either inline in main or a new shared location)
- Preserve all existing flags: `--model`, `--temperature`, `--max-tokens`, `--top-p`, `--output-format`, `--continue`, `--resume`, `--fork`, `--no-session`, `--worktree`, `--providers`
- Implement the error message: "No input provided. Provide a prompt as an argument, use 'acai -' for stdin, or pipe input to acai."
- Ensure `acai "prompt"` works end-to-end with all existing functionality (model selection, worktrees, sessions, etc.)

This slice delivers the core `acai "your prompt here"` experience while keeping all other features intact.

## Acceptance criteria

- [ ] Running `acai "test prompt"` executes successfully without the `instruct` subcommand
- [ ] All existing flags (`--model`, `--temperature`, etc.) work with the new syntax
- [ ] Error message displays when no prompt is provided and stdin is empty
- [ ] Session management (--continue, --resume, --fork) works with new syntax
- [ ] Worktree functionality (-w/--worktree) works with new syntax
- [ ] Output formats (--output-format text|stream-json) work with new syntax
- [ ] Tests pass for the refactored CLI structure

## Blocked by

None - can start immediately

## User stories addressed

- User story 1: Direct prompt execution without `instruct --prompt`
- User story 7: All existing flags continue working
- User story 8: Old `instruct` command removed
- User story 10: Clear error message when no input provided
