// Pre-execution safety checks for the Bash tool.
//
// Blocks known-destructive commands that operate within the sandbox's allowed
// zone (e.g. destructive git operations inside the repo) or affect remote
// state (e.g. force-push). This is a best-effort guard, not a security
// boundary — the OS-level sandbox remains the primary enforcement layer.

// =============================================================================
// Public API
// =============================================================================

/// Validate that a command does not contain known-destructive operations.
/// Returns `Ok(())` if safe, or `Err(formatted_message)` if blocked.
pub(super) fn validate_command_safety(command: &str) -> Result<(), String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    // Split on top-level command separators and check each segment
    for segment in split_segments(trimmed) {
        let seg = segment.trim();
        if seg.is_empty() {
            continue;
        }

        // Check for shell -c wrappers and recurse into the inner script
        if let Some(inner) = extract_inline_script(seg) {
            validate_command_safety(&inner)?;
            continue;
        }

        let normalized = normalize_whitespace(seg);
        let lower = normalized.to_lowercase();

        // Skip segments that are commit/tag messages (data, not commands)
        if is_data_context(&lower) {
            continue;
        }

        check_git_reset(&lower, seg)?;
        check_git_checkout(&lower, seg)?;
        check_git_restore(&lower, seg)?;
        check_git_clean(&lower, seg)?;
        check_git_push(&lower, seg)?;
        check_git_branch_delete(&normalized, seg)?;
        check_git_stash(&lower, seg)?;
        check_dangerous_rm(&normalized, seg)?;
    }

    Ok(())
}

// =============================================================================
// Error Formatting
// =============================================================================

fn blocked(reason: &str, command: &str, tip: &str) -> String {
    format!("BLOCKED\n\nReason: {reason}\n\nCommand: {command}\n\nTip: {tip}")
}

// =============================================================================
// Helpers
// =============================================================================

/// Collapse all whitespace runs to a single space.
fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split a command string on top-level separators (`;`, `&&`, `||`, `|`, `\n`).
/// Tracks single and double quotes so that separators inside quoted strings
/// are not treated as split points.
fn split_segments(command: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let bytes = command.as_bytes();
    let len = bytes.len();
    let mut start = 0;
    let mut i = 0;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while i < len {
        // Track quote state (skip escaped quotes in double-quote context)
        if bytes[i] == b'\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            i += 1;
            continue;
        }
        if bytes[i] == b'"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            i += 1;
            continue;
        }
        if bytes[i] == b'\\' && in_double_quote && i + 1 < len {
            i += 2; // skip escaped char
            continue;
        }

        // Only split when outside quotes
        if !in_single_quote && !in_double_quote {
            if bytes[i] == b'\n' || bytes[i] == b';' {
                segments.push(&command[start..i]);
                start = i + 1;
                i += 1;
                continue;
            }
            if i + 1 < len && bytes[i] == b'&' && bytes[i + 1] == b'&' {
                segments.push(&command[start..i]);
                start = i + 2;
                i += 2;
                continue;
            }
            if i + 1 < len && bytes[i] == b'|' && bytes[i + 1] == b'|' {
                segments.push(&command[start..i]);
                start = i + 2;
                i += 2;
                continue;
            }
            // Single pipe — also a command boundary
            if bytes[i] == b'|' {
                segments.push(&command[start..i]);
                start = i + 1;
                i += 1;
                continue;
            }
        }

        i += 1;
    }

    if start < len {
        segments.push(&command[start..]);
    }

    segments
}

/// Check if a (lowercased, normalized) command is in a data context where
/// destructive-looking text is not actually executed (e.g. commit messages).
fn is_data_context(lower: &str) -> bool {
    lower.starts_with("git commit")
        || lower.starts_with("git tag")
        || lower.starts_with("echo ")
        || lower.starts_with("printf ")
        || lower.starts_with("cat >")
        || lower.starts_with("cat >>")
        || lower.starts_with("tee ")
}

/// Extract the inner script from `bash -c "..."` or `sh -c "..."`.
/// Returns `None` if the command is not a shell -c invocation.
/// Handles flexible whitespace between the shell name and `-c`.
fn extract_inline_script(command: &str) -> Option<String> {
    let normalized = normalize_whitespace(command);
    let lower = normalized.to_lowercase();

    // Match: bash -c or sh -c (with optional leading env/path)
    let idx = lower.find("bash -c ").or_else(|| lower.find("sh -c "))?;
    let shell_len = if lower[idx..].starts_with("bash") {
        8 // len("bash -c ")
    } else {
        6 // len("sh -c ")
    };

    // Map back to original command: count the corresponding characters
    // by finding the same position accounting for whitespace normalization
    let after_flag = skip_to_after_flag(command, idx, shell_len)?;

    let trimmed = after_flag.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Strip surrounding quotes if present
    let first = trimmed.as_bytes()[0];
    if (first == b'"' || first == b'\'') && trimmed.len() > 1 {
        let quote = first;
        // Find matching closing quote (not escaped)
        let inner = &trimmed[1..];
        let mut end = None;
        let bytes = inner.as_bytes();
        let mut j = 0;
        while j < bytes.len() {
            if bytes[j] == b'\\' {
                j += 2;
                continue;
            }
            if bytes[j] == quote {
                end = Some(j);
                break;
            }
            j += 1;
        }
        if let Some(e) = end {
            return Some(inner[..e].to_string());
        }
    }

    // Unquoted — take everything
    Some(trimmed.to_string())
}

/// Map a position in the whitespace-normalized string back to the original,
/// then skip past `shell_len` normalized tokens to find where the script starts.
fn skip_to_after_flag(original: &str, norm_pos: usize, shell_len: usize) -> Option<&str> {
    // Count how many non-whitespace characters precede norm_pos in the normalized string
    let normalized = normalize_whitespace(original);
    let prefix = &normalized[..norm_pos + shell_len];
    let token_count = prefix.split_whitespace().count();

    // Walk the original string, skipping that many whitespace-separated tokens
    let mut seen = 0;
    let mut i = 0;
    let bytes = original.as_bytes();
    while i < bytes.len() && seen < token_count {
        // Skip whitespace
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        // Skip token
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        seen += 1;
    }

    (i <= original.len()).then(|| &original[i..])
}

// =============================================================================
// Git Checks
// =============================================================================

/// `git reset --hard` / `git reset --merge`
fn check_git_reset(lower: &str, original: &str) -> Result<(), String> {
    if lower.contains("git reset --hard") || lower.contains("git reset --merge") {
        return Err(blocked(
            "git reset --hard/--merge destroys uncommitted changes",
            original,
            "Use 'git stash' to save changes first, or 'git reset --soft' to preserve them.",
        ));
    }
    Ok(())
}

/// `git checkout -- <file>`
fn check_git_checkout(lower: &str, original: &str) -> Result<(), String> {
    if lower.contains("git checkout --") {
        // Verify there's a path after `--`
        if let Some(pos) = lower.find("git checkout --") {
            let after = lower[pos + 15..].trim(); // len("git checkout --")
            if !after.is_empty() && !after.starts_with('-') {
                return Err(blocked(
                    "git checkout -- <file> discards uncommitted file changes",
                    original,
                    "Use 'git restore --staged <file>' to unstage, or 'git stash' to save changes.",
                ));
            }
        }
    }
    Ok(())
}

/// `git restore <file>` without `--staged`, or with `--worktree`
/// `git restore -b <branch>` is allowed (creates a branch).
fn check_git_restore(lower: &str, original: &str) -> Result<(), String> {
    if !lower.contains("git restore") {
        return Ok(());
    }

    // --worktree is always destructive, even with --staged
    if lower.contains("--worktree") {
        return Err(blocked(
            "git restore --worktree discards uncommitted changes",
            original,
            "Use 'git restore --staged <file>' to only unstage, or 'git stash' to save changes.",
        ));
    }

    // --staged alone is safe (only unstages)
    if lower.contains("--staged") {
        return Ok(());
    }

    // Find what comes after `git restore`
    if let Some(pos) = lower.find("git restore") {
        let after = lower[pos + 11..].trim(); // len("git restore")
        // -b creates a branch, not destructive
        if after.is_empty() || after.starts_with("-b") || after.starts_with("-b ") {
            return Ok(());
        }
        // Bare `git restore <file>` without --staged is destructive
        if !after.is_empty() {
            return Err(blocked(
                "git restore <file> without --staged discards uncommitted changes",
                original,
                "Use 'git restore --staged <file>' to only unstage, or 'git stash' to save changes.",
            ));
        }
    }

    Ok(())
}

/// `git clean -f` / `--force` — includes combined flags like `-fd`, `-fdx`
fn check_git_clean(lower: &str, original: &str) -> Result<(), String> {
    if !lower.contains("git clean") {
        return Ok(());
    }

    if lower.contains("git clean --force") || lower.contains("git clean -f") {
        return Err(blocked(
            "git clean -f permanently deletes untracked files",
            original,
            "Use 'git clean -n' to preview what would be deleted first.",
        ));
    }

    // Check for combined flags containing 'f', e.g. `-fd`, `-xfd`, `-fdx`
    if let Some(pos) = lower.find("git clean") {
        let after = lower[pos + 9..].trim(); // len("git clean")
        // Look for a dash-flag group containing 'f'
        for token in after.split_whitespace() {
            if token.starts_with('-') && !token.starts_with("--") && token.contains('f') {
                return Err(blocked(
                    "git clean -f permanently deletes untracked files",
                    original,
                    "Use 'git clean -n' to preview what would be deleted first.",
                ));
            }
        }
    }

    Ok(())
}

/// `git push --force` / `-f` — allows `--force-with-lease`
fn check_git_push(lower: &str, original: &str) -> Result<(), String> {
    if !lower.contains("git push") {
        return Ok(());
    }

    // --force-with-lease is safe
    if lower.contains("--force-with-lease") {
        return Ok(());
    }

    if lower.contains("git push --force") {
        return Err(blocked(
            "git push --force overwrites remote commit history",
            original,
            "Use 'git push --force-with-lease' for safer force pushes.",
        ));
    }

    // Check for short flag -f (but not part of a longer flag group that isn't force)
    if let Some(pos) = lower.find("git push") {
        let after = lower[pos + 8..].trim(); // len("git push")
        for token in after.split_whitespace() {
            if token == "-f" {
                return Err(blocked(
                    "git push -f overwrites remote commit history",
                    original,
                    "Use 'git push --force-with-lease' for safer force pushes.",
                ));
            }
            // Combined flags like -fu, -uf
            if token.starts_with('-')
                && !token.starts_with("--")
                && token.len() > 1
                && token.contains('f')
            {
                return Err(blocked(
                    "git push -f overwrites remote commit history",
                    original,
                    "Use 'git push --force-with-lease' for safer force pushes.",
                ));
            }
        }
    }

    Ok(())
}

/// `git branch -D` — uppercase D only (force delete without merge check).
/// Uses the original (case-preserved, whitespace-normalized) string.
fn check_git_branch_delete(normalized: &str, original: &str) -> Result<(), String> {
    // We need case-insensitive match for "git branch" but case-sensitive for the flag.
    let lower = normalized.to_lowercase();
    if !lower.contains("git branch") {
        return Ok(());
    }

    // Find "git branch" case-insensitively, then inspect the flag in the original
    let search = "git branch";
    let lower_bytes = lower.as_bytes();
    let mut i = 0;
    while i + search.len() <= lower_bytes.len() {
        if &lower[i..i + search.len()] == search {
            let after = normalized[i + search.len()..].trim_start();
            // Check the first token for a flag containing uppercase D
            if let Some(token) = after.split_whitespace().next()
                && token.starts_with('-')
                && !token.starts_with("--")
            {
                let flags = &token[1..];
                if flags.chars().any(|c| c == 'D') {
                    return Err(blocked(
                        "git branch -D force-deletes branches without checking merge status",
                        original,
                        "Use 'git branch -d' (lowercase) to safely delete only merged branches.",
                    ));
                }
            }
        }
        i += 1;
    }

    Ok(())
}

/// `git stash drop` / `git stash clear`
fn check_git_stash(lower: &str, original: &str) -> Result<(), String> {
    if lower.contains("git stash drop") || lower.contains("git stash clear") {
        return Err(blocked(
            "git stash drop/clear permanently deletes stashed changes",
            original,
            "Use 'git stash list' to review stashes, or 'git stash pop' to apply and remove.",
        ));
    }
    Ok(())
}

// =============================================================================
// rm -rf Check
// =============================================================================

/// Block `rm -rf` targeting obviously dangerous paths.
/// Allowed: `/tmp/*`, `/var/tmp/*`, `$TMPDIR/*`, `${TMPDIR}/*`.
fn check_dangerous_rm(normalized: &str, original: &str) -> Result<(), String> {
    let lower = normalized.to_lowercase();
    if !lower.contains("rm ") {
        return Ok(());
    }

    // Find `rm` invocations with `-rf` or `-fr` (including combined flags)
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    for (i, &tok) in tokens.iter().enumerate() {
        if tok.eq_ignore_ascii_case("rm") {
            // Collect all flag tokens after rm
            let mut has_r = false;
            let mut has_f = false;
            let mut target_start = i + 1;

            for (j, &flag_tok) in tokens.iter().enumerate().skip(i + 1) {
                if flag_tok.starts_with('-') && !flag_tok.starts_with("--") {
                    let flags = &flag_tok[1..];
                    if flags.contains('r') || flags.contains('R') {
                        has_r = true;
                    }
                    if flags.contains('f') || flags.contains('F') {
                        has_f = true;
                    }
                    target_start = j + 1;
                } else {
                    break;
                }
            }

            if has_r && has_f && target_start < tokens.len() {
                let target = tokens[target_start];
                if !is_allowed_rm_target(target) {
                    return Err(blocked(
                        "rm -rf outside of temporary directories can cause permanent data loss",
                        original,
                        "rm -rf is only allowed for /tmp/*, /var/tmp/*, or $TMPDIR/* paths.",
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Check if an `rm -rf` target is in an allowed temporary directory.
fn is_allowed_rm_target(target: &str) -> bool {
    let allowed_prefixes = [
        "/tmp/",
        "/tmp",
        "/var/tmp/",
        "/var/tmp",
        "$TMPDIR/",
        "$TMPDIR",
        "${TMPDIR}/",
        "${TMPDIR}",
    ];

    for prefix in &allowed_prefixes {
        if target == *prefix || target.starts_with(&format!("{prefix}/")) {
            return true;
        }
        if target == *prefix {
            return true;
        }
    }

    false
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Shorthand: assert command is blocked
    fn assert_blocked(command: &str) {
        let result = validate_command_safety(command);
        assert!(result.is_err(), "Expected command to be BLOCKED: {command}");
    }

    // Shorthand: assert command is allowed
    fn assert_allowed(command: &str) {
        if let Err(msg) = validate_command_safety(command) {
            panic!("Expected command to be ALLOWED: {command}\nGot: {msg}");
        }
    }

    // =========================================================================
    // git reset
    // =========================================================================

    #[test]
    fn blocks_git_reset_hard() {
        assert_blocked("git reset --hard");
        assert_blocked("git reset --hard HEAD");
        assert_blocked("git reset --hard HEAD~3");
        assert_blocked("GIT RESET --HARD");
        assert_blocked("Git Reset --Hard");
    }

    #[test]
    fn blocks_git_reset_merge() {
        assert_blocked("git reset --merge");
    }

    #[test]
    fn allows_git_reset_soft() {
        assert_allowed("git reset --soft HEAD~1");
        assert_allowed("git reset HEAD~1");
    }

    // =========================================================================
    // git checkout
    // =========================================================================

    #[test]
    fn blocks_git_checkout_dash_dash_file() {
        assert_blocked("git checkout -- src/main.rs");
        assert_blocked("git checkout -- .");
    }

    #[test]
    fn allows_git_checkout_branch() {
        assert_allowed("git checkout main");
        assert_allowed("git checkout -b new-branch");
    }

    // =========================================================================
    // git restore
    // =========================================================================

    #[test]
    fn blocks_git_restore_file() {
        assert_blocked("git restore src/main.rs");
        assert_blocked("git restore .");
    }

    #[test]
    fn blocks_git_restore_worktree() {
        assert_blocked("git restore --worktree src/main.rs");
        assert_blocked("git restore --staged --worktree src/main.rs");
    }

    #[test]
    fn allows_git_restore_staged() {
        assert_allowed("git restore --staged src/main.rs");
    }

    #[test]
    fn allows_git_restore_branch() {
        assert_allowed("git restore -b new-branch");
    }

    // =========================================================================
    // git clean
    // =========================================================================

    #[test]
    fn blocks_git_clean_force() {
        assert_blocked("git clean -f");
        assert_blocked("git clean --force");
        assert_blocked("git clean -fd");
        assert_blocked("git clean -fdx");
        assert_blocked("git clean -xf");
    }

    #[test]
    fn allows_git_clean_dry_run() {
        assert_allowed("git clean -n");
        assert_allowed("git clean -nd");
    }

    // =========================================================================
    // git push
    // =========================================================================

    #[test]
    fn blocks_git_push_force() {
        assert_blocked("git push --force");
        assert_blocked("git push --force origin main");
        assert_blocked("git push -f");
        assert_blocked("git push -f origin main");
        assert_blocked("git push -fu origin main");
    }

    #[test]
    fn allows_git_push_force_with_lease() {
        assert_allowed("git push --force-with-lease");
        assert_allowed("git push --force-with-lease origin main");
    }

    #[test]
    fn allows_normal_git_push() {
        assert_allowed("git push");
        assert_allowed("git push origin main");
    }

    // =========================================================================
    // git branch -D
    // =========================================================================

    #[test]
    fn blocks_git_branch_uppercase_d() {
        assert_blocked("git branch -D feature-branch");
    }

    #[test]
    fn allows_git_branch_lowercase_d() {
        assert_allowed("git branch -d feature-branch");
    }

    // =========================================================================
    // git stash
    // =========================================================================

    #[test]
    fn blocks_git_stash_drop() {
        assert_blocked("git stash drop");
        assert_blocked("git stash drop stash@{0}");
    }

    #[test]
    fn blocks_git_stash_clear() {
        assert_blocked("git stash clear");
    }

    #[test]
    fn allows_git_stash_other() {
        assert_allowed("git stash");
        assert_allowed("git stash pop");
        assert_allowed("git stash list");
        assert_allowed("git stash apply");
    }

    // =========================================================================
    // rm -rf
    // =========================================================================

    #[test]
    fn blocks_rm_rf_dangerous_targets() {
        assert_blocked("rm -rf /");
        assert_blocked("rm -rf /home/user");
        assert_blocked("rm -rf ~/projects");
        assert_blocked("rm -rf .");
        assert_blocked("rm -rf ..");
        assert_blocked("rm -rf /usr");
        assert_blocked("rm -rf /etc");
    }

    #[test]
    fn allows_rm_rf_temp_dirs() {
        assert_allowed("rm -rf /tmp/build-cache");
        assert_allowed("rm -rf /var/tmp/test");
        assert_allowed("rm -rf $TMPDIR/foo");
        assert_allowed("rm -rf ${TMPDIR}/bar");
    }

    #[test]
    fn allows_rm_without_rf() {
        assert_allowed("rm file.txt");
        assert_allowed("rm -r dir/");
        assert_allowed("rm -f file.txt");
    }

    // =========================================================================
    // Inline scripts (bash -c / sh -c)
    // =========================================================================

    #[test]
    fn blocks_inline_script_with_destructive_command() {
        assert_blocked("bash -c \"git reset --hard\"");
        assert_blocked("sh -c \"git clean -f\"");
        assert_blocked("bash -c 'git push --force origin main'");
    }

    #[test]
    fn blocks_inline_script_with_extra_whitespace() {
        assert_blocked("bash  -c \"git reset --hard\"");
        assert_blocked("bash\t-c \"git reset --hard\"");
    }

    #[test]
    fn blocks_destructive_after_inline_script() {
        assert_blocked("bash -c 'echo safe' ; rm -rf /");
        assert_blocked("bash -c 'echo safe' && git reset --hard");
    }

    #[test]
    fn allows_inline_script_with_safe_command() {
        assert_allowed("bash -c \"echo hello\"");
        assert_allowed("sh -c \"git status\"");
    }

    // =========================================================================
    // Command chaining
    // =========================================================================

    #[test]
    fn blocks_destructive_in_chain() {
        assert_blocked("echo done && git reset --hard");
        assert_blocked("git add . && git reset --hard HEAD~1");
        assert_blocked("git stash; git reset --hard; git stash pop");
    }

    #[test]
    fn blocks_destructive_via_pipe() {
        // Pipe splits segments so each side is checked independently
        assert_blocked("git status | tee log.txt && git reset --hard");
        assert_blocked("git log | head -5 ; rm -rf /");
    }

    #[test]
    fn allows_safe_chain() {
        assert_allowed("git add . && git commit -m 'test'");
        assert_allowed("cargo build && cargo test");
    }

    // =========================================================================
    // False positive avoidance
    // =========================================================================

    #[test]
    fn allows_commit_message_containing_destructive_text() {
        assert_allowed("git commit -m \"fix: handle git branch -D correctly\"");
        assert_allowed("git commit -m \"docs: explain git reset --hard\"");
    }

    #[test]
    fn allows_echo_containing_destructive_text() {
        assert_allowed("echo \"git reset --hard\"");
    }

    #[test]
    fn allows_quoted_separators_in_commit_messages() {
        assert_allowed("git commit -m \"fix: updated stuff ; git reset --hard\"");
        assert_allowed("git commit -m 'refactor && git clean -f'");
    }

    // =========================================================================
    // Whitespace variations
    // =========================================================================

    #[test]
    fn handles_extra_whitespace() {
        assert_blocked("git  reset  --hard");
        assert_blocked("git\treset\t--hard");
    }

    // =========================================================================
    // Error message format
    // =========================================================================

    #[test]
    fn error_message_contains_reason_and_tip() {
        let Err(err) = validate_command_safety("git reset --hard") else {
            panic!("should be blocked");
        };
        assert!(err.contains("BLOCKED"), "Missing BLOCKED header");
        assert!(err.contains("Reason:"), "Missing reason");
        assert!(err.contains("Command:"), "Missing command");
        assert!(err.contains("Tip:"), "Missing tip");
    }
}
