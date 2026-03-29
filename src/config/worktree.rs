use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, anyhow};
use tracing::info;

/// Adjectives for random worktree name generation.
const ADJECTIVES: &[&str] = &[
    "bright", "calm", "cool", "dark", "fast", "keen", "pure", "soft", "warm", "wild", "bold",
    "crisp", "fair", "glad", "kind", "neat", "rare", "safe", "tall", "vast",
];

/// Nouns for random worktree name generation.
const NOUNS: &[&str] = &[
    "arch", "beam", "core", "dart", "edge", "flux", "gate", "hive", "iris", "jade", "kite", "leaf",
    "mesa", "node", "opal", "pine", "quay", "reef", "sage", "tide",
];

/// Represents an active git worktree managed by acai.
#[derive(Debug, Clone)]
pub struct Worktree {
    /// The name of the worktree (user-provided or auto-generated).
    pub name: String,
    /// The filesystem path to the worktree directory.
    pub path: PathBuf,
}

/// Generate a random worktree name like "bright-core-a1b2".
pub fn generate_name() -> String {
    let uuid = uuid::Uuid::new_v4();
    let bytes = uuid.as_bytes();

    let adj = ADJECTIVES[bytes[0] as usize % ADJECTIVES.len()];
    let noun = NOUNS[bytes[1] as usize % NOUNS.len()];
    let suffix = &hex::encode(&bytes[2..4])[..4];

    format!("{adj}-{noun}-{suffix}")
}

/// Find the root of the current git repository.
fn find_repo_root(from: &Path) -> anyhow::Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(from)
        .output()
        .context("Failed to run git rev-parse")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Not a git repository: {stderr}"));
    }

    let root = String::from_utf8(output.stdout)
        .context("Invalid UTF-8 in git output")?
        .trim()
        .to_string();

    Ok(PathBuf::from(root))
}

/// Get the default remote branch (e.g., origin/main).
fn default_remote_branch(repo_root: &Path) -> anyhow::Result<String> {
    // Try origin/HEAD first
    let output = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(repo_root)
        .output()
        .context("Failed to run git symbolic-ref")?;

    if output.status.success() {
        let full_ref = String::from_utf8(output.stdout)
            .context("Invalid UTF-8")?
            .trim()
            .to_string();
        // Strip "refs/remotes/" prefix
        if let Some(branch) = full_ref.strip_prefix("refs/remotes/") {
            return Ok(branch.to_string());
        }
    }

    // Fallback: try common defaults
    for candidate in &["origin/main", "origin/master"] {
        let check = Command::new("git")
            .args(["rev-parse", "--verify", candidate])
            .current_dir(repo_root)
            .output()
            .context("Failed to verify branch")?;

        if check.status.success() {
            return Ok((*candidate).to_string());
        }
    }

    // Last resort: use HEAD
    Ok("HEAD".to_string())
}

/// Returns the worktrees base directory: `<repo>/.acai/worktrees/`.
fn worktrees_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".acai").join("worktrees")
}

/// Create a new worktree with the given name. If `name` is `None`, a random name is generated.
///
/// The worktree is created at `<repo>/.acai/worktrees/<name>` with a new branch
/// `worktree-<name>` based on the default remote branch.
pub fn create(from: &Path, name: Option<&str>) -> anyhow::Result<Worktree> {
    let repo_root = find_repo_root(from)?;
    let wt_name = name.map_or_else(generate_name, ToString::to_string);
    let branch = format!("worktree-{wt_name}");
    let wt_path = worktrees_dir(&repo_root).join(&wt_name);

    if wt_path.exists() {
        return Err(anyhow!(
            "Worktree '{wt_name}' already exists at {}",
            wt_path.display()
        ));
    }

    let start_point = default_remote_branch(&repo_root)?;

    info!(
        "Creating worktree '{wt_name}' at {} (branch: {branch}, from: {start_point})",
        wt_path.display()
    );

    let output = Command::new("git")
        .args([
            "worktree",
            "add",
            &wt_path.to_string_lossy(),
            "-b",
            &branch,
            &start_point,
        ])
        .current_dir(&repo_root)
        .output()
        .context("Failed to run git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to create worktree: {stderr}"));
    }

    Ok(Worktree {
        name: wt_name,
        path: wt_path,
    })
}

/// Check if a worktree has uncommitted changes or commits ahead of its start point.
pub fn has_changes(wt_path: &Path) -> anyhow::Result<bool> {
    // Check for uncommitted changes (staged or unstaged)
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(wt_path)
        .output()
        .context("Failed to run git status")?;

    if !status.status.success() {
        return Err(anyhow!("Failed to check worktree status"));
    }

    let status_output = String::from_utf8_lossy(&status.stdout);
    if !status_output.trim().is_empty() {
        return Ok(true);
    }

    // Check for commits not on the default remote branch
    let log = Command::new("git")
        .args(["log", "--oneline", "@{upstream}..HEAD"])
        .current_dir(wt_path)
        .output();

    // If upstream doesn't exist, check if there are any commits at all
    match log {
        Ok(output) if output.status.success() => {
            let log_output = String::from_utf8_lossy(&output.stdout);
            Ok(!log_output.trim().is_empty())
        },
        _ => Ok(false),
    }
}

/// Remove a worktree by name. Deletes the worktree directory and its branch.
///
/// Set `force` to `true` to remove even if there are uncommitted changes.
pub fn remove(from: &Path, name: &str, force: bool) -> anyhow::Result<()> {
    let repo_root = find_repo_root(from)?;
    let wt_path = worktrees_dir(&repo_root).join(name);
    let branch = format!("worktree-{name}");

    if !wt_path.exists() {
        return Err(anyhow!("Worktree '{name}' not found"));
    }

    info!("Removing worktree '{name}' at {}", wt_path.display());

    // Remove the worktree
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    let wt_path_str = wt_path.to_string_lossy().to_string();
    args.push(&wt_path_str);

    let output = Command::new("git")
        .args(&args)
        .current_dir(&repo_root)
        .output()
        .context("Failed to run git worktree remove")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to remove worktree: {stderr}"));
    }

    // Delete the branch
    let branch_flag = if force { "-D" } else { "-d" };
    let branch_output = Command::new("git")
        .args(["branch", branch_flag, &branch])
        .current_dir(&repo_root)
        .output()
        .context("Failed to delete branch")?;

    if !branch_output.status.success() {
        let stderr = String::from_utf8_lossy(&branch_output.stderr);
        // Don't fail if branch was already deleted
        if !stderr.contains("not found") {
            tracing::warn!("Failed to delete branch '{branch}': {stderr}");
        }
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Create a temporary git repo for testing.
    fn init_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        // Configure git user for CI environments where global config may not exist
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "--allow-empty", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        dir
    }

    #[test]
    fn test_generate_name_format() {
        let name = generate_name();
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 3, "Name should have 3 parts: {name}");
        assert_eq!(parts[2].len(), 4, "Suffix should be 4 hex chars");
    }

    #[test]
    fn test_find_repo_root() {
        let dir = init_test_repo();
        let root = find_repo_root(dir.path()).unwrap();
        assert_eq!(
            root.canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn test_create_worktree() {
        let dir = init_test_repo();

        let wt = create(dir.path(), Some("test-wt")).unwrap();
        assert_eq!(wt.name, "test-wt");
        assert!(wt.path.exists());
    }

    #[test]
    fn test_create_duplicate_fails() {
        let dir = init_test_repo();
        create(dir.path(), Some("dup")).unwrap();
        let result = create(dir.path(), Some("dup"));
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_clean_worktree() {
        let dir = init_test_repo();
        let wt = create(dir.path(), Some("removable")).unwrap();
        assert!(wt.path.exists());

        remove(dir.path(), "removable", false).unwrap();
        assert!(!wt.path.exists());
    }

    #[test]
    fn test_has_changes_clean() {
        let dir = init_test_repo();
        let wt = create(dir.path(), Some("clean")).unwrap();
        assert!(!has_changes(&wt.path).unwrap());
    }

    #[test]
    fn test_has_changes_dirty() {
        let dir = init_test_repo();
        let wt = create(dir.path(), Some("dirty")).unwrap();

        // Create a new file to make it dirty
        std::fs::write(wt.path.join("new_file.txt"), "content").unwrap();
        assert!(has_changes(&wt.path).unwrap());
    }
}
