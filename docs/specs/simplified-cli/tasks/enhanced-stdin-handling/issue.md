## Parent PRD

[docs/specs/simplified-cli/prd.md](../../prd.md)

## What to build

Implement codex-style stdin handling that supports multiple input patterns for maximum flexibility.

Key changes:
- Support `-` as a special prompt value meaning "read entire prompt from stdin" (e.g., `echo "prompt" | acai -`)
- Handle piped stdin when no positional prompt is provided (e.g., `cat file.txt | acai`)
- Concatenate positional prompt with piped stdin when both are provided, with the prompt first followed by stdin content separated by a newline (e.g., `cat code.py | acai "review this"`)
- Support shell heredocs (e.g., `acai <<EOF ... EOF`)
- Support input redirection (e.g., `acai < prompt.txt`)

This slice enables all the flexible input methods developers expect from modern CLI tools.

## Acceptance criteria

- [ ] `acai -` reads entire prompt from stdin
- [ ] `cat file.txt | acai` works (no positional prompt, stdin provided)
- [ ] `cat file.txt | acai "instructions"` concatenates prompt + stdin content
- [ ] `acai < prompt.txt` works (input redirection)
- [ ] `acai <<EOF ... EOF` heredoc syntax works
- [ ] `(echo "instructions"; cat file.py) | acai` combined input works
- [ ] Unit tests cover all stdin input combinations
- [ ] Integration tests verify end-to-end stdin handling

## Blocked by

- #1 - Core CLI refactor with positional prompt

## User stories addressed

- User story 2: Pipe file content with instructions
- User story 3: Heredoc support for multi-line prompts
- User story 4: `acai -` reads from stdin
- User story 5: Input redirection support
- User story 6: Combine piped input with positional prompt
