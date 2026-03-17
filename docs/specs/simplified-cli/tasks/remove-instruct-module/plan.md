# Plan: Remove Instruct Module and --prompt-file Flag

## Parent Issue

[docs/specs/simplified-cli/tasks/remove-instruct-module/issue.md](./issue.md)  
Parent PRD: [docs/specs/simplified-cli/prd.md](../../prd.md)

---

## Context

The simplified CLI interface has been successfully implemented in `src/main.rs` via the `CodingAssistant` struct. This new interface accepts prompts as positional arguments at the root level (e.g., `acai "prompt"`), replacing the old `acai instruct --prompt "..."` syntax.

The old `instruct` subcommand module (`src/cli/cmds/instruct.rs`) and its `--prompt-file` flag are now **dead code** and need to be removed to complete the transition to the simplified CLI.

---

## Current State

| File | Purpose | Status |
|------|---------|--------|
| `src/main.rs` | New simplified CLI (`CodingAssistant`) | ✅ Active - handles positional prompt, stdin, all flags |
| `src/cli/cmds/instruct.rs` | Old `instruct` subcommand with `--prompt` and `--prompt-file` flags | 🗑️ Unused - to be deleted |
| `src/cli/cmds/mod.rs` | Module declarations | 🗑️ Only contains `pub mod instruct;` - to be emptied/removed |
| `src/cli/mod.rs` | CLI module exports | ✅ Keep - exports `cmds` and `CmdRunner` |

---

## Implementation

### Phase 1: Remove Instruct Module

**What:** Delete the unused `instruct` subcommand module.

**Steps:**

1. **Delete `src/cli/cmds/instruct.rs`**
   ```bash
   rm src/cli/cmds/instruct.rs
   ```

2. **Update `src/cli/cmds/mod.rs`**
   - Remove `pub mod instruct;`
   - Since this leaves the file empty, delete the entire file:
   ```bash
   rm src/cli/cmds/mod.rs
   ```

3. **Update `src/cli/mod.rs`**
   - Remove `pub mod cmds;` declaration
   - Keep `mod cmd_runner;` and `pub use cmd_runner::*;`

**Verification:**
```bash
# Check the files are removed/updated
cargo build --release 2>&1 | head -20
```

**Expected:** Clean build with no errors.

---

### Phase 2: Verify No Dead Code or References Remain

**What:** Search for any remaining references to `instruct` module or `--prompt-file` flag.

**Steps:**

1. **Search for "instruct" references:**
   ```bash
   grep -r "instruct" src/ --include="*.rs"
   ```
   
   **Expected result:** Only matches should be in comments or test names that refer to "instructions" (not the module). No `use crate::cli::cmds::instruct` or similar.

2. **Search for "prompt_file" or "prompt-file":**
   ```bash
   grep -r "prompt_file\|prompt-file" src/ --include="*.rs"
   ```
   
   **Expected result:** No matches (the flag only existed in the deleted `instruct.rs`).

3. **Check for unused imports:**
   ```bash
   cargo clippy -- -Wunused_imports 2>&1 | grep -E "(warning|error)" | head -20
   ```

**Verification:**
```bash
# Full build with strict warnings
cargo build --release
```

**Expected:** No warnings or errors.

---

### Phase 3: Run All Tests

**What:** Ensure all existing tests pass after cleanup.

**Steps:**

1. **Run unit tests:**
   ```bash
   cargo test
   ```

2. **Run with coverage (optional but recommended):**
   ```bash
   just coverage
   ```

**Expected:** All tests pass. No test failures related to removed code.

---

### Phase 4: Final Verification

**What:** Verify the CLI still works correctly after cleanup.

**Manual verification steps:**

1. **Build the release binary:**
   ```bash
   cargo build --release
   ```

2. **Test basic invocation:**
   ```bash
   ./target/release/acai --help
   ```
   
   **Expected:** Help shows `CodingAssistant` options, no `instruct` subcommand listed.

3. **Test positional prompt:**
   ```bash
   ./target/release/acai "echo hello"
   ```
   
   **Expected:** Command executes (or shows API key error if not set, which is expected).

4. **Verify `--prompt-file` flag is gone:**
   ```bash
   ./target/release/acai --prompt-file test.txt 2>&1
   ```
   
   **Expected:** Error message like "error: unexpected argument '--prompt-file' found".

---

## Success Criteria

### Automated Verification

| Criteria | Command | Expected Result |
|----------|---------|-----------------|
| Build passes | `cargo build --release` | Exit code 0, no errors |
| Tests pass | `cargo test` | All tests pass |
| No warnings | `cargo clippy` | No warnings about dead code |
| No instruct references | `grep -r "instruct" src/ --include="*.rs"` | Only unrelated matches (e.g., "instructions" in comments) |
| No prompt_file references | `grep -r "prompt_file" src/ --include="*.rs"` | No matches |

### Manual Verification

| Criteria | Steps | Expected Result |
|----------|-------|-----------------|
| `--prompt-file` removed | Run `acai --prompt-file test.txt` | Error: unexpected argument |
| No `instruct` subcommand | Run `acai instruct --help` | Error: unexpected argument 'instruct' |
| Positional prompt works | Run `acai "test prompt"` | Executes (or API key error) |
| Stdin still works | `echo "test" \| acai` | Processes stdin correctly |

---

## Out of Scope

The following are **NOT** part of this task:

- Adding new flags or features
- Modifying the `CodingAssistant` behavior in `main.rs`
- Changing test logic (tests should continue to work as-is)
- Documentation updates (covered in task #4)
- README updates (covered in task #4)

---

## Rollback Plan

If issues are discovered:

1. Restore files from git:
   ```bash
   git checkout -- src/cli/cmds/instruct.rs src/cli/cmds/mod.rs
   ```

2. Restore `src/cli/mod.rs`:
   ```bash
   git checkout -- src/cli/mod.rs
   ```

3. Verify build:
   ```bash
   cargo build --release
   ```

---

## Decision Log

| Date | Decision | Reason |
|------|----------|--------|
| 2026-03-17 | Delete entire `cmds/` subdirectory | The `instruct` module was the only command; with it gone, the `cmds/` module is empty and unnecessary |

---

## Progress

- [x] Phase 1: Remove instruct module
- [x] Phase 2: Verify no dead code
- [x] Phase 3: Run all tests
- [x] Phase 4: Final verification

---

*Last updated: 2026-03-17*
