# Implementation Plan: Core CLI Refactor with Positional Prompt

**Parent PRD:** [../../prd.md](../../prd.md)  
**Task Issue:** [issue.md](issue.md)  
**Status:** Planned  
**Created:** 2026-03-16

## 1. Goal

Transform the CLI from a subcommand-based interface (`acai instruct --prompt "..."`) to a simplified interface that accepts prompts as positional arguments at the root level (`acai "your prompt here"`). This is the foundational change that enables the simplified CLI experience while preserving all existing functionality.

**Success Criteria:**
- `acai "test prompt"` executes successfully without the `instruct` subcommand
- All existing flags work with the new syntax
- Clear error message when no input is provided
- All tests pass

## 2. Current State

The current CLI structure (as of `src/main.rs`):

```rust
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct CodingAssistant {
    #[command(subcommand)]
    pub cmd: CodingAssistantCmd,
}

#[derive(Clone, Subcommand)]
enum CodingAssistantCmd {
    Instruct(instruct::Cmd),
}
```

The `instruct::Cmd` struct (in `src/cli/cmds/instruct.rs:24-66`) contains:
- `--model`, `--temperature`, `--max-tokens`, `--top-p` flags
- `--prompt` and `--prompt-file` arguments (to be removed)
- Session management flags: `--continue`, `--resume`, `--fork`, `--no-session`
- `--worktree` flag for isolated git worktrees
- `--output-format` for text vs stream-json output
- `--providers` for provider restrictions

Current invocation: `acai instruct --prompt "refactor this"` or `acai instruct -p "refactor this"`

## 3. Target State

The new CLI structure:

```rust
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct CodingAssistant {
    /// The prompt to send to the AI
    #[arg(value_name = "PROMPT")]
    pub prompt: Option<String>,
    
    // All existing flags preserved at root level
    #[arg(long, default_value = DEFAULT_MODEL)]
    pub model: String,
    // ... other flags
}
```

Target invocation: `acai "refactor this"` with all flags working the same way.

Key behaviors:
- `acai "prompt"` → executes with the provided prompt
- `acai -` → reads entire prompt from stdin
- `acai` with piped stdin → reads from stdin when not a TTY
- `acai "instructions"` with piped stdin → concatenates prompt + stdin
- No input at all → displays error message

## 4. Implementation

### Phase 1: Refactor CLI Structure in main.rs

**Objective:** Transform the CLI from subcommand-based to flat with positional argument.

**Steps:**

1. **Replace the CLI struct definitions** in `src/main.rs:12-26`:
   - Remove `CodingAssistantCmd` enum entirely
   - Replace `CodingAssistant` with a flat struct containing all flags plus positional prompt
   - Keep all existing flags with identical names and behavior

2. **Define the new CLI struct**:
   ```rust
   #[derive(Parser)]
   #[command(author, version, about, long_about = None)]
   struct CodingAssistant {
       /// The prompt to send to the AI (use `-` to read from stdin)
       #[arg(value_name = "PROMPT")]
       pub prompt: Option<String>,

       #[arg(long, default_value = DEFAULT_MODEL)]
       pub model: String,

       #[arg(long)]
       pub temperature: Option<f32>,

       #[arg(long)]
       pub max_tokens: Option<u32>,

       #[arg(long)]
       pub top_p: Option<f32>,

       #[arg(long, value_enum, default_value = "text")]
       pub output_format: OutputFormat,

       #[arg(long = "continue")]
       pub continue_session: bool,

       #[arg(long, value_name = "UUID")]
       pub resume: Option<String>,

       #[arg(long, num_args = 0..=1, default_missing_value = "", value_name = "UUID")]
       pub fork: Option<String>,

       #[arg(long)]
       pub no_session: bool,

       #[arg(short, long, num_args = 0..=1, default_missing_value = "", value_name = "NAME")]
       pub worktree: Option<String>,

       #[arg(long, num_args = 0.., value_delimiter = ',')]
       pub providers: Vec<String>,
   }
   ```

3. **Move `OutputFormat` enum** from `src/cli/cmds/instruct.rs:15-23` to `src/main.rs` (or import it).

4. **Update `should_use_quiet_logging()`** function in `src/main.rs:29-42`:
   - Change from checking `args.cmd` to checking `args.output_format` directly
   - The logic remains: return true if `output_format == OutputFormat::StreamJson`

5. **Update the main execution logic** in `src/main.rs:55-59`:
   - Remove the `match args.cmd` block
   - Call the execution logic directly with `args`

**Phase 1 Success Criteria:**
- [x] [AUTOMATED] Code compiles: `cargo build --release`
- [x] [AUTOMATED] No clippy warnings: `just clippy-strict`
- [x] [MANUAL] `cargo run --release -- --help` shows the new CLI structure with positional PROMPT
- [x] [MANUAL] `cargo run --release -- "test prompt"` attempts to run (will fail on execution logic, which is Phase 2)

---

### Phase 2: Implement Execution Logic

**Objective:** Move or adapt the execution logic from `instruct::Cmd` to work with the new `CodingAssistant` struct.

**Steps:**

1. **Extract shared execution logic** (choose one approach):
   
   **Option A: Move logic to main.rs (recommended for this refactor)**
   - Copy the `run()` method body from `src/cli/cmds/instruct.rs:230-332` into a new async function in `main.rs`
   - Adapt references from `self` to `args` (the `CodingAssistant` struct)
   
   **Option B: Create a shared runner module**
   - Create `src/cli/runner.rs` with the execution logic
   - Have both old and new structures call it during transition

2. **Implement stdin handling logic** in the main execution:
   ```rust
   // Pseudocode for stdin handling:
   let stdin_content = if !std::io::stdin().is_terminal() {
       std::io::read_to_string(std::io::stdin()).ok()
   } else {
       None
   };

   let content = match (args.prompt.as_deref(), stdin_content) {
       (Some("-"), Some(stdin)) => stdin,           // acai - < input.txt
       (Some("-"), None) => {                       // acai - (with no piped input)
           return Err(anyhow::anyhow!("No input provided via stdin"));
       },
       (Some(prompt), Some(stdin)) => format!("{}\n\n{}", prompt, stdin), // Both
       (Some(prompt), None) => prompt.to_string(),  // Just prompt
       (None, Some(stdin)) => stdin,                // Just stdin
       (None, None) => {                            // Nothing at all
           return Err(anyhow::anyhow!(
               "No input provided. Provide a prompt as an argument, use 'acai -' for stdin, or pipe input to acai."
           ));
       },
   };
   ```

3. **Copy/adapt helper methods** from `instruct::Cmd`:
   - `build_client_and_session()` (lines 75-134)
   - `setup_worktree()` (lines 136-155)
   - `cleanup_worktree()` (lines 157-180)

4. **Update imports in main.rs** to include all required types:
   - `use crate::cli::instruct::OutputFormat;` (or move the enum)
   - `use crate::clients::Responses;`
   - `use crate::models::{Message, Role};`
   - `use crate::prompts::build_system_prompt;`
   - `use crate::config::{AgentsFile, DEFAULT_MODEL, Session, worktree};`

**Phase 2 Success Criteria:**
- [x] [AUTOMATED] `cargo build --release` compiles successfully
- [x] [AUTOMATED] `cargo test` passes
- [x] [MANUAL] `acai "hello"` runs end-to-end (requires OPENROUTER_API_KEY)
- [x] [MANUAL] `acai` with no args shows the correct error message
- [x] [MANUAL] `echo "test" | acai` reads from stdin
- [x] [MANUAL] `echo "context" | acai "prompt"` combines both

---

### Phase 3: Remove instruct Module Dependency

**Objective:** Clean up references to the old `instruct` module from main.rs.

**Steps:**

1. **Remove unused import** in `src/main.rs`:
   - Remove `use cli::instruct;`

2. **Verify `OutputFormat` is accessible**:
   - Either keep it in `src/cli/cmds/instruct.rs` and re-export it
   - Or move it to `src/main.rs` or a shared location

3. **Update `src/cli/cmds/mod.rs`** if needed (may be addressed in a later task).

**Phase 3 Success Criteria:**
- [AUTOMATED] `cargo build --release` compiles with no warnings about unused imports
- [AUTOMATED] `cargo test` passes
- [MANUAL] `acai --help` works correctly

---

### Phase 4: Add Unit Tests for CLI Parsing

**Objective:** Add tests to verify the new argument parsing behavior.

**Steps:**

1. **Add a test module** at the bottom of `src/main.rs`:
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_cli_parsing_positional_prompt() {
           let args = CodingAssistant::parse_from(["acai", "test prompt"]);
           assert_eq!(args.prompt, Some("test prompt".to_string()));
       }

       #[test]
       fn test_cli_parsing_with_flags() {
           let args = CodingAssistant::parse_from([
               "acai",
               "--model", "gpt-4",
               "--temperature", "0.5",
               "prompt here"
           ]);
           assert_eq!(args.prompt, Some("prompt here".to_string()));
           assert_eq!(args.model, "gpt-4");
           assert_eq!(args.temperature, Some(0.5));
       }

       #[test]
       fn test_cli_parsing_dash_for_stdin() {
           let args = CodingAssistant::parse_from(["acai", "-"]);
           assert_eq!(args.prompt, Some("-".to_string()));
       }

       #[test]
       fn test_cli_parsing_no_prompt() {
           let args = CodingAssistant::parse_from(["acai"]);
           assert_eq!(args.prompt, None);
       }
   }
   ```

2. **Run tests** to verify parsing works correctly.

**Phase 4 Success Criteria:**
- [x] [AUTOMATED] `cargo test` passes including new unit tests
- [x] [AUTOMATED] Test coverage for CLI parsing is comprehensive

---

### Phase 5: Integration Verification

**Objective:** Verify end-to-end functionality with all flag combinations.

**Manual Test Checklist:**

1. **Basic execution:**
   ```bash
   export OPENROUTER_API_KEY=your_key
   cargo run --release -- "What is 2+2?"
   ```

2. **All flags work:**
   ```bash
   cargo run --release -- --model anthropic/claude-3.5-sonnet "test"
   cargo run --release -- --temperature 0.7 --max-tokens 100 "test"
   cargo run --release -- --output-format stream-json "test"
   ```

3. **Session management:**
   ```bash
   cargo run --release -- --continue "follow up question"
   cargo run --release -- --no-session "one-off query"
   ```

4. **Worktree functionality:**
   ```bash
   cargo run --release -- -w "test in worktree"
   cargo run --release -- --worktree my-wt "named worktree"
   ```

5. **Stdin handling:**
   ```bash
   echo "context" | cargo run --release -- "process this"
   echo "full prompt" | cargo run --release --
   cargo run --release -- - <<< "heredoc content"
   ```

6. **Error cases:**
   ```bash
   cargo run --release --  # Should show error message
   ```

**Phase 5 Success Criteria:**
- [MANUAL] All manual test cases pass
- [MANUAL] Error message displays correctly when no input provided
- [MANUAL] All existing functionality preserved

---

## 5. Verification

### Automated Verification Commands

```bash
# Build and basic checks
cargo build --release
just clippy-strict
cargo fmt --check

# Run all tests
cargo test

# Check the help output shows the new structure
cargo run --release -- --help
```

### Manual Verification Steps

1. **Verify new syntax works:**
   ```bash
   $ cargo run --release -- "Say hello"
   # Should output a greeting
   ```

2. **Verify old syntax is rejected:**
   ```bash
   $ cargo run --release -- instruct --prompt "test"
   # Should show error: unexpected argument 'instruct' found
   ```

3. **Verify error message:**
   ```bash
   $ cargo run --release --
   # Should show: No input provided. Provide a prompt as an argument...
   ```

4. **Verify all flags:**
   ```bash
   $ cargo run --release -- --help | grep -E "\-\-(model|temperature|max-tokens|top-p|output-format|continue|resume|fork|no-session|worktree|providers)"
   # Should show all flags present
   ```

---

## 6. Rollback Plan

If issues are discovered:

1. **Immediate rollback:** Revert to the previous commit
   ```bash
   git checkout HEAD~1 -- src/main.rs
   ```

2. **Partial rollback:** Keep new structure but restore `instruct` subcommand temporarily:
   - Add back `CodingAssistantCmd` enum with `Instruct` variant
   - Support both `acai "prompt"` and `acai instruct --prompt "..."` during transition

---

## 7. Open Questions

None - all requirements are clear from the PRD and codebase analysis.

---

## 8. Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2026-03-16 | Move execution logic to main.rs | Simplifies the refactor; the old `instruct` module will be fully removed in task #3 anyway |
| 2026-03-16 | Keep `OutputFormat` in main.rs | It's CLI-specific and needed for `should_use_quiet_logging()` |

---

## 9. Appendix

### File References

- Current CLI entry: `src/main.rs:1-63`
- Current instruct command: `src/cli/cmds/instruct.rs:1-332`
- CmdRunner trait: `src/cli/cmd_runner.rs:1-29`

### Flag Mapping

| Old Flag | New Location | Behavior Change |
|----------|--------------|-----------------|
| `instruct --prompt "text"` | positional arg `"text"` | Now positional, not a flag |
| `instruct --prompt-file file` | Removed | Use `acai < file` instead |
| `instruct --model` | `--model` | Unchanged |
| `instruct --temperature` | `--temperature` | Unchanged |
| `instruct --max-tokens` | `--max-tokens` | Unchanged |
| `instruct --top-p` | `--top-p` | Unchanged |
| `instruct --output-format` | `--output-format` | Unchanged |
| `instruct --continue` | `--continue` | Unchanged |
| `instruct --resume` | `--resume` | Unchanged |
| `instruct --fork` | `--fork` | Unchanged |
| `instruct --no-session` | `--no-session` | Unchanged |
| `instruct --worktree` | `--worktree` / `-w` | Unchanged |
| `instruct --providers` | `--providers` | Unchanged |
