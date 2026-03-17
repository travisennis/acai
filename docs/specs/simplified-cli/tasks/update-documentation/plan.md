# Update Documentation and Examples for Simplified CLI

This plan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

## Purpose / Big Picture

Update all user-facing documentation to reflect the new simplified CLI interface where `acai "prompt"` replaces `acai instruct --prompt "prompt"`. This ensures users can discover and correctly use the new interface patterns.

What someone can do after this change that they could not before:
- Find accurate usage examples in README.md showing positional prompts
- Discover stdin piping patterns for combining prompts with file content
- Understand how to use heredocs for multi-line prompts
- See the correct syntax for all flags (no `--prompt` or `--prompt-file`)

How to verify it works:
- All documentation files show only `acai "prompt"` syntax (no `instruct` references)
- README.md includes clear stdin, heredoc, and redirection examples
- `cargo run -- --help` shows the updated help text (no `--prompt` flag)

## Progress

- [x] (Completed) Milestone 1: Update README.md
- [x] (Completed) Milestone 2: Update AGENTS.md
- [x] (Completed) Milestone 3: Update docs/references/responses-api.md
- [x] (Completed) Milestone 4: Update docs/design-docs/session-management.md
- [x] (Completed) Milestone 5: Update prompt.md and verify help output
- [x] (Completed) Final verification and cleanup

## Surprises & Discoveries

*None yet - to be filled during implementation*

## Decision Log

*None yet - to be filled during implementation*

## Outcomes & Retrospective

### What Was Completed

Successfully updated all user-facing documentation to reflect the new simplified CLI interface:

1. **README.md** - Complete rewrite of Usage section with:
   - Positional prompt examples
   - Stdin piping examples (`cat file | acai "summarize"`)
   - Heredoc examples (both standalone and with prompt prefix)
   - Input redirection examples (`acai < file.txt`)
   - Updated session management examples
   - Updated worktrees examples
   - Updated Options section (removed --prompt/--prompt-file)

2. **AGENTS.md** - Updated "Running the App" section (3 examples)

3. **docs/references/responses-api.md** - Updated CLI Usage section (3 examples)

4. **docs/design-docs/session-management.md** - Updated:
   - Overview paragraph
   - 5 code examples
   - Directory Isolation examples
   - Implementation Details file reference

5. **prompt.md** - Updated Phase 1 instructions (2 references)

6. **Additional files discovered and updated**:
   - `docs/design-docs/streaming-json-output.md`
   - `docs/design-docs/sandbox.md`
   - `docs/design-docs/cli.md` (complete rewrite)

### Verification Results

All automated checks pass:
- ✅ No `acai instruct` references in user-facing docs
- ✅ No `--prompt-file` references in user-facing docs
- ✅ No problematic `--prompt` references in user-facing docs
- ✅ Build succeeds: `cargo build --release`
- ✅ Tests pass: `cargo test`
- ✅ Formatting OK: `cargo fmt --check`
- ✅ Linting passes: `cargo clippy`
- ✅ Help output shows positional `[PROMPT]` argument
- ✅ Help output has no `--prompt` or `--prompt-file` options

### Files Modified

- `README.md`
- `AGENTS.md`
- `prompt.md`
- `docs/references/responses-api.md`
- `docs/design-docs/session-management.md`
- `docs/design-docs/streaming-json-output.md`
- `docs/design-docs/sandbox.md`
- `docs/design-docs/cli.md`

### Lessons Learned

- The grep-based verification was essential to catch missed references
- Design docs in `docs/design-docs/` are user-facing and need to be kept in sync
- The plan's scope was slightly under-estimated - always do a comprehensive grep first

## Context and Orientation

The simplified CLI refactor (tasks #1-3) has removed the `instruct` subcommand and `--prompt`/`--prompt-file` flags. The CLI now uses a positional argument for the prompt with automatic stdin detection.

### Key Terms:
- **Positional prompt**: The prompt text provided as a bare argument: `acai "write code"`
- **Stdin placeholder**: Using `-` as the prompt to explicitly read from stdin: `acai - < file.txt`
- **Heredoc**: Bash syntax for multi-line input: `acai << 'EOF'`
- **Input redirection**: Shell syntax to read file content as stdin: `acai < file.txt`

### Key Files:
- `README.md`: Primary user documentation - needs complete overhaul of usage examples
- `AGENTS.md`: Development guide - needs updating in "Running the App" section
- `docs/references/responses-api.md`: API reference - needs CLI usage updates
- `docs/design-docs/session-management.md`: Session documentation - needs example updates
- `prompt.md`: Internal evaluation prompt - needs CLI usage updates
- `src/main.rs`: Contains the actual CLI structure (reference only, already complete)

## What We're NOT Doing

- **CHANGELOG.md**: Historical record, preserved as-is
- **rfh.md / rfh-findings.md**: Research files for other evaluation purposes
- **CLI code changes**: Already complete from tasks #1-3
- **Adding new features**: This is documentation-only, no code changes
- **Rewriting architecture docs**: ARCHITECTURE.md references the `instruct` module but describes the architecture accurately; minimal updates only if needed

## Implementation Approach

The approach is systematic file-by-file updates:

1. **README.md** (most important): Complete rewrite of usage section with new examples
2. **AGENTS.md**: Quick update to "Running the App" section
3. **docs/references/responses-api.md**: Update CLI Usage section
4. **docs/design-docs/session-management.md**: Update all session examples
5. **prompt.md**: Update Phase 1 instructions
6. **Final verification**: Run `--help` and grep for any remaining outdated references

Each file update follows the same pattern:
- Replace `acai instruct --prompt` with `acai`
- Replace `acai instruct -p` with `acai`
- Remove `--prompt-file` references
- Add new stdin/heredoc/redirection examples where appropriate

## Milestones

### Milestone 1: Update README.md

**Overview**: Rewrite the Usage section of README.md to show the new simplified CLI syntax with positional prompts, stdin piping, heredocs, and input redirection.

**Repository Context**: File: `README.md` at repository root.

**Plan of Work**:

1. Update the "Usage" section header to remove "Instruct Mode" subsection
2. Replace all examples in the usage code block:
   - Change `acai instruct --prompt "..."` → `acai "..."`
   - Remove the `--prompt-file` example
   - Add new stdin piping example: `cat file.txt | acai "summarize"`
   - Add new heredoc example (prompt only): `acai << 'EOF'`
   - Add new heredoc with prompt prefix: `acai "Review:" << 'EOF'`
   - Add input redirection: `acai < prompt.txt`
   - Add stdin placeholder: `acai - < file.txt`
3. Update Session Management examples:
   - Change `acai instruct --prompt` → `acai`
   - Change `acai instruct --continue --prompt` → `acai --continue`
   - Change `acai instruct --resume` → `acai --resume`
4. Update Worktrees examples:
   - Change `acai instruct -w feature-auth -p` → `acai -w feature-auth`
5. Update Options section:
   - Remove `--prompt` and `--prompt-file` documentation
   - Note that prompt is now positional
6. Update final Example section:
   - Change `acai instruct --model --prompt` → `acai --model`

**Interfaces and Dependencies**: None - documentation only.

**Concrete Steps**:

Working directory: /Users/travisennis/Projects/acai

```bash
# Preview current README.md usage section
cat README.md | head -n 100
```

Make edits to `README.md` as described above. The key diff should look like:

```diff
-### Instruct Mode
-
-Generate code or documentation based on instructions:
+## Usage
 
 ```bash
 # Basic usage with a prompt
-acai instruct --prompt "Implement a binary search tree in Rust"
+acai "Implement a binary search tree in Rust"
 
-# Read prompt from a file (avoids shell escaping issues)
-acai instruct --prompt-file ./my-prompt.txt
+# Pipe file content with instructions
+cat src/main.rs | acai "Explain this code"
+
+# Use a heredoc for multi-line prompts
+acai << 'EOF'
+Implement a function that:
+1. Takes a list of numbers
+2. Returns the sum
+EOF
+
+# Heredoc with prompt prefix
+acai "Review this code:" << 'EOF'
+fn main() {
+    println!("Hello");
+}
+EOF
+
+# Input redirection
+acai < prompt.txt
+
+# Read from stdin explicitly
+acai - < file.txt
 ```
```

**Validation and Acceptance**:

#### Automated Verification:
- [ ] File exists and is valid markdown: `test -f README.md`
- [ ] No `instruct` references remain: `! grep -n "acai instruct" README.md`
- [ ] No `--prompt` flag references remain: `! grep -n "\-\-prompt" README.md`
- [ ] No `--prompt-file` references remain: `! grep -n "prompt-file" README.md`

#### Manual Verification:
- [ ] Open README.md and verify the Usage section reads correctly
- [ ] Verify all examples use `acai "prompt"` syntax
- [ ] Verify stdin piping example is clear
- [ ] Verify heredoc examples are clear and both styles are shown
- [ ] Verify Options section no longer mentions `--prompt` or `--prompt-file`

**Idempotence and Recovery**: This is a documentation edit. To rollback: `git checkout README.md`

**Artifacts and Evidence**: Include the final README.md Usage section as evidence.

---

### Milestone 2: Update AGENTS.md

**Overview**: Update the "Running the App" section in AGENTS.md to use the new simplified CLI syntax.

**Repository Context**: File: `AGENTS.md` at repository root, lines 51-65 (Running the App section).

**Plan of Work**:

1. Find the "Running the App" section
2. Update the three examples:
   - `./target/release/acai instruct --prompt` → `./target/release/acai`
   - `cargo run --release -- instruct --prompt` → `cargo run --release --`
   - `./target/release/acai instruct --help` → `./target/release/acai --help`

**Concrete Steps**:

Working directory: /Users/travisennis/Projects/acai

```bash
# Preview current section
grep -n -A 10 "Running the App" AGENTS.md
```

Edit `AGENTS.md` - the changes should look like:

```diff
 # Run binary directly
-./target/release/acai instruct --prompt "Your prompt here"
+./target/release/acai "Your prompt here"
 
 # Or with cargo
-cargo run --release -- instruct --prompt "Your prompt here"
+cargo run --release -- "Your prompt here"
 
 # To get help
-./target/release/acai instruct --help
+./target/release/acai --help
```

**Validation and Acceptance**:

#### Automated Verification:
- [ ] No `instruct` references in AGENTS.md: `! grep -n "instruct" AGENTS.md`
- [ ] No `--prompt` flag references: `! grep -n "\-\-prompt" AGENTS.md`

#### Manual Verification:
- [ ] Verify all three examples use correct new syntax

---

### Milestone 3: Update docs/references/responses-api.md

**Overview**: Update the CLI Usage section in the Responses API reference to show the new simplified syntax.

**Repository Context**: File: `docs/references/responses-api.md`, CLI Usage section near end of file.

**Plan of Work**:

1. Find the "CLI Usage" section (around line 140)
2. Update all three examples:
   - `./target/release/acai instruct --prompt` → `./target/release/acai`
   - `./target/release/acai instruct --prompt --output-format` → `./target/release/acai --output-format`
   - `./target/release/acai instruct --model --prompt` → `./target/release/acai --model`

**Concrete Steps**:

Working directory: /Users/travisennis/Projects/acai

```bash
# Preview current section
grep -n -A 15 "CLI Usage" docs/references/responses-api.md
```

Edit `docs/references/responses-api.md` - changes should look like:

```diff
 ## CLI Usage
 
 ```bash
 # Set API key
 export OPENROUTER_API_KEY=your_key_here
 
 # Basic usage (text output)
-./target/release/acai instruct --prompt "Your prompt here"
+./target/release/acai "Your prompt here"
 
 # Streaming JSON output
-./target/release/acai instruct --prompt "Your prompt here" --output-format stream-json
+./target/release/acai "Your prompt here" --output-format stream-json
 
 # With options
-./target/release/acai instruct \
+./target/release/acai \
     --model "minimax/minimax-m2.5" \
     --temperature 0.7 \
     --max-tokens 4000 \
-    --prompt "Explain Rust ownership"
+    "Explain Rust ownership"
 ```
```

**Validation and Acceptance**:

#### Automated Verification:
- [ ] No `instruct` references: `! grep -n "instruct" docs/references/responses-api.md`
- [ ] No `--prompt` flag references: `! grep -n "\-\-prompt" docs/references/responses-api.md`

---

### Milestone 4: Update docs/design-docs/session-management.md

**Overview**: Update all session management examples to use the new simplified CLI syntax.

**Repository Context**: File: `docs/design-docs/session-management.md`, Usage section.

**Plan of Work**:

1. Find the "Usage" section
2. Update all examples:
   - `acai instruct --prompt` → `acai`
   - `acai instruct --continue --prompt` → `acai --continue`
   - `acai instruct --resume --prompt` → `acai --resume`
   - `acai instruct --no-session --prompt` → `acai --no-session`
3. Update the "Overview" paragraph that mentions `acai instruct`

**Concrete Steps**:

Working directory: /Users/travisennis/Projects/acai

```bash
# Preview all occurrences
grep -n "instruct" docs/design-docs/session-management.md
```

Edit `docs/design-docs/session-management.md` - changes should look like:

```diff
-Every time you run `acai instruct`, a session is automatically created...
+Every time you run `acai`, a session is automatically created...
 
 ### Starting a Session
 
-Every `acai instruct` invocation creates a new session automatically:
+Every `acai` invocation creates a new session automatically:
 
 ```bash
-acai instruct --prompt "My favorite color is blue"
+acai "My favorite color is blue"
 ```
 
 ### Continuing the Latest Session
 
 ```bash
-acai instruct --continue --prompt "What's my favorite color?"
+acai --continue "What's my favorite color?"
 ```
 
 ### Resuming a Specific Session
 
 ```bash
-acai instruct --resume 550e8400-e29b-41d4-a716-446655440000 --prompt "Continue our conversation"
+acai --resume 550e8400-e29b-41d4-a716-446655440000 "Continue our conversation"
 ```
 
 ### Disabling Session Saving
 
 ```bash
-acai instruct --no-session --prompt "Quick one-off question"
+acai --no-session "Quick one-off question"
 ```
```

**Validation and Acceptance**:

#### Automated Verification:
- [ ] No `instruct` references: `! grep -n "instruct" docs/design-docs/session-management.md`
- [ ] No `--prompt` flag references: `! grep -n "\-\-prompt" docs/design-docs/session-management.md`

---

### Milestone 5: Update prompt.md and Verify Help Output

**Overview**: Update the internal evaluation prompt and verify the `--help` output is accurate.

**Repository Context**: File: `prompt.md` at repository root.

**Plan of Work**:

1. Find Phase 1 in `prompt.md`
2. Update the CLI examples:
   - `./target/release/acai instruct` → `./target/release/acai`
   - `./target/release/acai instruct --prompt` → `./target/release/acai`
3. Verify `--help` output is accurate by running the command

**Concrete Steps**:

Working directory: /Users/travisennis/Projects/acai

```bash
# Preview occurrences in prompt.md
grep -n "instruct" prompt.md
```

Edit `prompt.md` - changes should look like:

```diff
-2. Use the acai CLI to accomplish the task. You must use `./target/release/acai instruct` to complete the work.
+2. Use the acai CLI to accomplish the task. You must use `./target/release/acai` to complete the work.
 
-   - Run the CLI: `./target/release/acai instruct --prompt "Your detailed prompt here"`
+   - Run the CLI: `./target/release/acai "Your detailed prompt here"`
```

Then verify the help output:

```bash
# Build first to ensure binary exists
cargo build --release 2>/dev/null || true

# Check help output
./target/release/acai --help
```

Verify that:
- No `--prompt` option is listed
- No `--prompt-file` option is listed
- The positional `[PROMPT]` argument is documented
- The description mentions stdin support

**Validation and Acceptance**:

#### Automated Verification:
- [ ] No `instruct` references in prompt.md: `! grep -n "instruct" prompt.md`
- [ ] Help output doesn't contain `--prompt`: `! ./target/release/acai --help | grep -q "\-\-prompt"`
- [ ] Help output shows positional PROMPT: `./target/release/acai --help | grep -q "PROMPT"`

#### Manual Verification:
- [ ] Run `./target/release/acai --help` and verify it looks correct
- [ ] Verify the help text explains that `-` reads from stdin
- [ ] Verify the help text documents the positional prompt argument

---

### Milestone 6: Final Verification and Cleanup

**Overview**: Do a final comprehensive check across all markdown files to catch any missed references.

**Concrete Steps**:

Working directory: /Users/travisennis/Projects/acai

```bash
# Check for any remaining instruct references in markdown
echo "=== Checking for 'acai instruct' references ==="
grep -rn "acai instruct" *.md docs/

echo "=== Checking for --prompt references (excluding code blocks about XML/tool prompts) ==="
grep -rn "\-\-prompt" *.md docs/ | grep -v "todo.*prompt" | grep -v "context.*prompt"

echo "=== Checking for --prompt-file references ==="
grep -rn "prompt-file" *.md docs/

echo "=== Building and testing ==="
cargo build --release
cargo test
```

**Validation and Acceptance**:

#### Automated Verification:
- [ ] No `acai instruct` references in any markdown: `! grep -rn "acai instruct" *.md docs/`
- [ ] No `--prompt-file` references: `! grep -rn "prompt-file" *.md docs/`
- [ ] Build succeeds: `cargo build --release` exits 0
- [ ] Tests pass: `cargo test` exits 0
- [ ] Formatting passes: `cargo fmt -- --check` exits 0
- [ ] Linting passes: `cargo clippy --all-targets -- -D warnings` exits 0

#### Manual Verification:
- [ ] Review each updated file (README.md, AGENTS.md, responses-api.md, session-management.md, prompt.md)
- [ ] Verify examples are clear and consistent
- [ ] Verify the help output looks good

**Idempotence and Recovery**: All changes are documentation edits tracked by git. Full rollback: `git checkout -- '*.md' docs/`

## Testing Strategy

### Unit Tests:
- N/A - documentation only task

### Integration Tests:
- N/A - documentation only task

### Manual Testing Steps:
1. Build the project: `cargo build --release`
2. Run help to verify output: `./target/release/acai --help`
3. Test each example from README.md manually:
   - `./target/release/acai "echo hello"`
   - `echo "test" | ./target/release/acai -`
   - `./target/release/acai < somefile.txt`

## Performance Considerations

N/A - documentation only task.

## Migration Notes

Users coming from the old CLI will need to update their scripts:
- `acai instruct --prompt "text"` → `acai "text"`
- `acai instruct --prompt-file file.txt` → `acai < file.txt`

The documentation changes in README.md will help users understand the new patterns.

## Rollback Plan

Since this is a documentation-only change, rollback is simple:

```bash
# Restore all markdown files from git
git checkout -- README.md AGENTS.md prompt.md
git checkout -- docs/references/responses-api.md docs/design-docs/session-management.md
```

## References

- Issue: `docs/specs/simplified-cli/tasks/update-documentation/issue.md`
- PRD: `docs/specs/simplified-cli/prd.md`
- Task index: `docs/specs/simplified-cli/tasks/index.md`
- Related plans:
  - Task #1: `docs/specs/simplified-cli/tasks/core-cli-refactor/plan.md`
  - Task #2: `docs/specs/simplified-cli/tasks/enhanced-stdin-handling/plan.md`
  - Task #3: `docs/specs/simplified-cli/tasks/remove-instruct-module/plan.md`

---

## Revision History

*To be filled as plan evolves*
