## Parent PRD

[docs/specs/simplified-cli/prd.md](../../prd.md)

## What to build

Update all documentation to reflect the new simplified CLI interface and provide clear examples of the new usage patterns.

Key changes:
- Update README.md:
  - Replace all `acai instruct --prompt "..."` examples with `acai "..."`
  - Replace all `acai instruct -p "..."` examples with `acai "..."`
  - Add examples for stdin piping: `cat file.txt | acai "summarize"`
  - Add examples for heredocs
  - Add examples for input redirection: `acai < prompt.txt`
  - Add example for `acai -` reading from stdin
  - Update the "Instruct Mode" section title and content
- Check and update AGENTS.md if it contains usage examples
- Review and update any other markdown files with CLI examples
- Verify `--help` output is accurate and helpful

This slice ensures users can discover and understand the new interface.

## Acceptance criteria

- [ ] README.md shows only new `acai "prompt"` syntax (no `instruct` references)
- [ ] README.md includes stdin piping examples
- [ ] README.md includes heredoc examples
- [ ] README.md includes input redirection examples
- [ ] README.md includes `acai -` example
- [ ] AGENTS.md updated if needed
- [ ] No references to `--prompt` or `--prompt-file` flags remain in docs
- [ ] `--help` output is accurate and helpful

## Blocked by

- #1 - Core CLI refactor with positional prompt
- #2 - Enhanced stdin handling
- #3 - Remove instruct module and --prompt-file

## User stories addressed

- All user stories (documentation support)
