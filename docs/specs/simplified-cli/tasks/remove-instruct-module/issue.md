## Parent PRD

[docs/specs/simplified-cli/prd.md](../../prd.md)

## What to build

Clean up the codebase by removing the deprecated `instruct` subcommand and `--prompt-file` flag since the new simplified interface replaces them.

Key changes:
- Delete `src/cli/cmds/instruct.rs` module entirely
- Remove `instruct` module declaration from `src/cli/cmds/mod.rs`
- Remove `--prompt-file` flag (replaced by stdin redirection: `acai < file.txt`)
- Clean up any instruct-specific types, structs, or exports that are no longer needed
- Update `src/cli/mod.rs` if needed
- Ensure no dead code remains in the CLI module

This slice completes the removal of the old interface, leaving only the new simplified syntax.

## Acceptance criteria

- [ ] `src/cli/cmds/instruct.rs` file is deleted
- [ ] `instruct` module removed from `src/cli/cmds/mod.rs`
- [ ] `--prompt-file` flag no longer exists in CLI arguments
- [ ] All references to `instruct` subcommand removed from codebase
- [ ] No dead code or unused imports remain
- [ ] Build passes without warnings
- [ ] All tests pass

## Blocked by

- #1 - Core CLI refactor with positional prompt

## User stories addressed

- User story 8: Old `instruct` command removed (clean break, not deprecated)
