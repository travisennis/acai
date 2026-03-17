# Code Review: Simplified CLI Interface

## Overview

Refactors the CLI from a subcommand pattern (`acai instruct --prompt "..."`) to a flat positional-argument pattern (`acai "..."`), adds codex-style stdin handling, removes the `instruct` module and `--prompt-file` flag, and updates all documentation.

**Files Changed:** 23
**Lines Added:** 2805
**Lines Removed:** 477

**Review Scope:**
- All changes on `feat/simplified-cli` vs `master`
- PRD compliance (docs/specs/simplified-cli/prd.md)

## Summary

The refactoring is well-executed—faithful migration of every field and method from `instruct::Cmd` into `CodingAssistant`, clean stdin logic, thorough test coverage, and comprehensive doc updates. CI passes: build, clippy-strict, fmt, 157 unit tests + 5 integration tests all green.

**Overall Assessment:** Positive
**Recommendation:** Approve with comments

## Important Suggestions

### 1. `build_content` inconsistency with empty stdin
**Location:** `src/main.rs:104` and `src/main.rs:119`
**Category:** Correctness
**Description:** `read_stdin_content()` filters out empty strings (`.filter(|s| !s.is_empty())`), so empty stdin is converted to `None` before reaching `build_content`. However, `build_content` itself still accepts `Some("")` and the test `test_build_content_prompt_with_empty_stdin` asserts that `("my prompt", Some(""))` produces `"my prompt\n\n"` (trailing separator). This means the unit test exercises a path that's unreachable in production, and the result (`"my prompt\n\n"`) is arguably wrong—it should be `"my prompt"`.
**Rationale:** The intent is clear from `read_stdin_content`, but the divergence between the filter-at-read-time and the unguarded match arm creates a subtle correctness gap if `build_content` is ever called directly.
**Suggested Approach:** Either (a) add a `.filter(|s| !s.is_empty())` guard inside `build_content` so the prompt+empty-stdin arm produces just the prompt, or (b) update the test assertion to document the trailing `\n\n` as intentional. Option (a) is cleaner.

### 2. `CmdRunner` trait is now single-implementer boilerplate
**Location:** `src/cli/cmd_runner.rs:1-32`, `src/main.rs:253`
**Category:** Maintainability
**Description:** With `instruct::Cmd` removed, `CodingAssistant` is the sole implementer of `CmdRunner`. The trait + separate module adds indirection without benefit.
**Rationale:** The PRD mentions future subcommands, so keeping the trait is reasonable for forward-compatibility. This is a non-blocking observation—the trait costs almost nothing.
**Suggested Approach:** Keep as-is; revisit if no second command materializes.

### 3. Integration tests depend on a release binary
**Location:** `tests/stdin_handling.rs:12-16`
**Category:** Testing
**Description:** `get_binary_path()` hardcodes `target/release/acai`. If tests run without `--release` (e.g., `cargo test`), the binary may be stale or absent. The tests currently pass because the release binary was already built, but `cargo test` does not rebuild release binaries.
**Rationale:** A CI pipeline running `cargo test` without a prior `cargo build --release` would skip or fail these tests silently (15s elapsed suggests the binary exists).
**Suggested Approach:** Use `env!("CARGO_BIN_EXE_acai")` (available with `[[bin]]` targets) or `assert_cmd` crate's `Command::cargo_bin("acai")` to get the test-profile binary automatically.

## Minor Improvements

### 1. Design doc struct definition drifts from actual code
**Location:** `docs/design-docs/cli.md:98-137`
**Category:** Documentation
**Description:** The struct definition in the design doc adds `Debug, Clone` derives and uses `Option<String>` for model, `Option<OutputFormat>` for output_format, and `Option<Option<String>>` for worktree—none of which match the actual code (which uses `String`, `OutputFormat`, and `Option<String>` respectively).
**Suggested Change:** Sync the doc's code block with the actual `CodingAssistant` definition, or remove the code block and describe fields in prose.

### 2. Streaming-json doc still references `shorthand -p`
**Location:** `docs/design-docs/streaming-json-output.md:11-12`
**Category:** Documentation
**Description:** The second code block reads `acai --output-format stream-json "Your prompt here"` but the comment says "Or with shorthand:" — there's no shorthand difference, both blocks are identical after the PR.
**Suggested Change:** Remove the duplicate "shorthand" block since `-p` no longer exists.

## Positive Aspects

- Clean, mechanical migration—no logic changes to session management, worktree, or API client code.
- `build_content` is well-structured with clear match arms mapping to each stdin scenario (`src/main.rs:109-127`).
- Comprehensive unit tests for all `build_content` paths including edge cases (`src/main.rs:375-518`).
- All documentation (README, AGENTS.md, design docs, session-management, streaming-json, sandbox, responses-api) consistently updated—no stale `instruct` references in `src/`.
- Error message for no-input case is actionable and matches the PRD requirement (`src/main.rs:124`).
- Integration tests validate actual CLI behavior including help output, version, and error cases (`tests/stdin_handling.rs`).

## Code Quality Analysis

### Correctness & Logic
- All codex-style stdin rules implemented correctly: `-` reads stdin, piped stdin without prompt works, prompt+stdin concatenates with `\n\n` separator.
- Session flag mutual-exclusivity check preserved from the original.
- Edge case: empty string prompt (`""`) is accepted as valid—reasonable for piping scenarios.

### Project Conventions
- Follows existing patterns: `CmdRunner` trait, `anyhow` for errors, `clap` derive macros, `log` for warnings.
- Import style matches crate conventions (absolute `crate::` paths).

### Performance
- No concerns. `read_to_string(stdin())` is appropriate for one-shot CLI use.

### Test Coverage
- Unit tests: 14 tests covering all `build_content` branches and CLI parsing.
- Integration tests: 5 tests covering help, version, positional prompt, dash, and no-input error.
- Missing: no test for `--continue`/`--resume`/`--fork` mutual exclusivity (was it tested before?), no test for `--worktree` flag parsing. These are pre-existing gaps, not regressions.

### Security
- No secrets exposed. No new attack surface. stdin handling is safe.

### Documentation & Maintainability
- All docs updated. Minor drift in `cli.md` code block (see Minor #1).
- `main.rs` is now 519 lines—getting large but still cohesive. Could extract `CodingAssistant` impl to a separate file in the future.

## Risk Assessment & Deployment Considerations

- **Risk level:** Low
- **Deployment notes:** Breaking change for existing users—`acai instruct --prompt` no longer works. Binary is a CLI tool so users update on their own schedule.
- **Breaking changes:** `instruct` subcommand removed; `--prompt` and `--prompt-file` flags removed.

## Manual Testing Recommendations

1. `acai "hello world"` — basic positional prompt
2. `echo "explain this" | acai` — piped stdin without prompt
3. `echo "code here" | acai "review this"` — prompt + stdin combination
4. `acai -` with no pipe — should error with "No input provided via stdin"
5. `acai` with no input — should error with actionable message
6. `acai --continue "follow up"` — session continuation still works
7. `acai -w "do something"` — worktree flag still works

## References

- PRD: `docs/specs/simplified-cli/prd.md`
- Branch: `feat/simplified-cli` (8 commits, 36d64f3..244d33b)
