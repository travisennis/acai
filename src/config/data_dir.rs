use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, anyhow};
use sha2::{Digest, Sha256};

use super::Session;

/// Represents an AGENTS.md file with its path and content.
#[derive(Debug, Clone)]
pub struct AgentsFile {
    /// Display path (e.g., "~/.acai/AGENTS.md" or "./AGENTS.md")
    pub path: String,
    /// Content of the file
    pub content: String,
}

#[derive(Debug, Clone)]
/// Represents a data directory structure.
pub struct DataDir {
    /// The path to the data directory.
    data_dir: PathBuf,
}

impl DataDir {
    /// Creates a new instance of the struct.
    ///
    /// This function initializes a new instance by determining the user's home directory
    /// and creating a specific data directory within it. If the home directory cannot be found,
    /// the function will panic with an appropriate message. Similarly, if the data directory
    /// cannot be created, the function will also panic.
    ///
    /// # Returns
    /// A new instance of the struct with the `data_dir` field set to the created data directory.
    ///
    /// # Panics
    /// This function will panic if:
    /// - The home directory cannot be found.
    /// - The data directory cannot be created.
    ///
    /// # Example
    /// ```
    /// let instance = DataDir::new();
    /// ```
    pub fn new() -> anyhow::Result<Self> {
        let home_dir = dirs::home_dir();
        if let Some(home) = home_dir {
            let data_dir = home.join(".cache").join("acai");

            if !data_dir.exists() {
                fs::create_dir_all(&data_dir)?;
            }

            Ok(Self { data_dir })
        } else {
            Err(anyhow!("Could not create data directory."))
        }
    }

    pub fn get_cache_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    /// Returns the sessions directory path: `~/.cache/acai/sessions/`
    fn sessions_dir(&self) -> PathBuf {
        self.data_dir.join("sessions")
    }

    /// Hash a working directory path to a short hex string for directory naming.
    fn dir_hash(working_dir: &Path) -> String {
        let mut hasher = Sha256::new();
        hasher.update(working_dir.to_string_lossy().as_bytes());
        let result = hasher.finalize();
        // First 16 hex characters (8 bytes)
        hex::encode(&result[..8])
    }

    /// Save a session to `sessions/{dir_hash}/{session.id}.jsonl` with atomic write
    /// and update the `latest` reference.
    pub fn save_session(&self, session: &Session) -> anyhow::Result<PathBuf> {
        uuid::Uuid::parse_str(&session.id).map_err(|e| {
            let id = &session.id;
            anyhow!("Invalid session UUID '{id}': {e}")
        })?;

        let dir_hash = Self::dir_hash(&session.working_dir);
        let session_dir = self.sessions_dir().join(&dir_hash);
        let session_path = session_dir.join(format!("{}.jsonl", session.id));

        session.save(&session_path)?;

        // Update latest reference atomically
        Self::update_latest(&session_dir, &session.id)?;

        Ok(session_path)
    }

    /// Update the "latest" reference to point to the given session ID.
    /// Uses symlinks on Unix, a marker file on Windows.
    #[cfg(unix)]
    fn update_latest(session_dir: &Path, session_id: &str) -> anyhow::Result<()> {
        use std::os::unix::fs::symlink;

        let latest_link = session_dir.join("latest");
        let temp_link = session_dir.join(".latest_tmp");
        let target = format!("{session_id}.jsonl");

        // Remove temp symlink if it exists
        let _ = fs::remove_file(&temp_link);
        symlink(&target, &temp_link)
            .with_context(|| format!("Failed to create temp symlink at {}", temp_link.display()))?;
        fs::rename(&temp_link, &latest_link)
            .with_context(|| format!("Failed to rename symlink to {}", latest_link.display()))?;

        Ok(())
    }

    /// Update the "latest" reference to point to the given session ID (Windows fallback).
    #[cfg(not(unix))]
    fn update_latest(session_dir: &Path, session_id: &str) -> anyhow::Result<()> {
        let latest_file = session_dir.join("latest");
        let temp_file = session_dir.join(".latest_tmp");

        // Write to temp file first, then atomically rename
        fs::write(&temp_file, session_id)
            .with_context(|| format!("Failed to write temp file at {}", temp_file.display()))?;
        fs::rename(&temp_file, &latest_file)
            .with_context(|| format!("Failed to rename to {}", latest_file.display()))?;

        Ok(())
    }

    /// Load the most recent session for a given working directory.
    pub fn load_latest_session(&self, working_dir: &Path) -> anyhow::Result<Option<Session>> {
        let dir_hash = Self::dir_hash(working_dir);
        let latest_path = self.sessions_dir().join(&dir_hash).join("latest");

        if !latest_path.exists() {
            return Ok(None);
        }

        let session_id = Self::read_latest_session_id(&latest_path)?;
        let session_path = self
            .sessions_dir()
            .join(&dir_hash)
            .join(format!("{session_id}.jsonl"));

        if !session_path.exists() {
            return Ok(None);
        }

        Session::load(&session_path).map(Some)
    }

    /// Read the latest session ID from the latest reference (symlink or file).
    #[cfg(unix)]
    fn read_latest_session_id(latest_path: &Path) -> anyhow::Result<String> {
        let target = fs::read_link(latest_path)
            .with_context(|| format!("Failed to read symlink: {}", latest_path.display()))?;

        target
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.trim_end_matches(".jsonl").to_string())
            .ok_or_else(|| anyhow!("Invalid symlink target: {}", target.display()))
    }

    /// Read the latest session ID from the latest reference (Windows file-based).
    #[cfg(not(unix))]
    fn read_latest_session_id(latest_path: &Path) -> anyhow::Result<String> {
        let content = fs::read_to_string(latest_path)
            .with_context(|| format!("Failed to read latest file: {}", latest_path.display()))?;
        Ok(content.trim().to_string())
    }

    /// Load a specific session by UUID, scoped to a working directory.
    pub fn load_session(&self, working_dir: &Path, id: &str) -> anyhow::Result<Option<Session>> {
        uuid::Uuid::parse_str(id).map_err(|e| anyhow!("Invalid session UUID '{id}': {e}"))?;

        let dir_hash = Self::dir_hash(working_dir);
        let session_path = self
            .sessions_dir()
            .join(&dir_hash)
            .join(format!("{id}.jsonl"));

        if !session_path.exists() {
            return Ok(None);
        }

        Session::load(&session_path).map(Some)
    }

    /// Read AGENTS.md files from user-level and project-level locations.
    ///
    /// Returns a list of found AGENTS.md files with their paths and content.
    /// Files that don't exist are silently skipped.
    pub fn read_agents_files(&self, working_dir: &Path) -> Vec<AgentsFile> {
        let mut files = Vec::new();

        // User-level AGENTS.md: ~/.acai/AGENTS.md
        let user_agents_path = self.data_dir.parent()
            .and_then(|p| p.parent()) // ~/.cache/acai -> ~/.cache -> ~
            .map(|p| p.join(".acai").join("AGENTS.md"))
            .or_else(|| dirs::home_dir().map(|h| h.join(".acai").join("AGENTS.md")));

        if let Some(ref path) = user_agents_path
            && let Ok(content) = fs::read_to_string(path)
        {
            files.push(AgentsFile {
                path: "~/.acai/AGENTS.md".to_string(),
                content,
            });
        }

        // Project-level AGENTS.md: ./AGENTS.md
        let project_agents_path = working_dir.join("AGENTS.md");
        if let Ok(content) = fs::read_to_string(&project_agents_path) {
            files.push(AgentsFile {
                path: "./AGENTS.md".to_string(),
                content,
            });
        }

        files
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_data_dir() -> (DataDir, TempDir) {
        let tmp = TempDir::new().unwrap();
        let dd = DataDir {
            data_dir: tmp.path().to_path_buf(),
        };
        (dd, tmp)
    }

    #[test]
    fn dir_hash_deterministic() {
        let path = PathBuf::from("/test/path");
        assert_eq!(DataDir::dir_hash(&path), DataDir::dir_hash(&path));
    }

    #[test]
    fn dir_hash_different_paths_differ() {
        let a = DataDir::dir_hash(&PathBuf::from("/a"));
        let b = DataDir::dir_hash(&PathBuf::from("/b"));
        assert_ne!(a, b);
    }

    #[test]
    fn dir_hash_output_format() {
        let path = PathBuf::from("/test/path");
        let hash = DataDir::dir_hash(&path);
        // Should be 16 hex characters (8 bytes)
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn save_and_load_session_round_trip() {
        let (dd, _tmp) = test_data_dir();
        let session = Session::new(uuid::Uuid::new_v4().to_string(), PathBuf::from("/work"));
        dd.save_session(&session).unwrap();
        let loaded = dd
            .load_session(&PathBuf::from("/work"), &session.id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.working_dir, session.working_dir);
    }

    #[test]
    fn save_and_load_latest_session() {
        let (dd, _tmp) = test_data_dir();
        let session = Session::new(uuid::Uuid::new_v4().to_string(), PathBuf::from("/work"));
        dd.save_session(&session).unwrap();
        let latest = dd
            .load_latest_session(&PathBuf::from("/work"))
            .unwrap()
            .unwrap();
        assert_eq!(latest.id, session.id);
    }

    #[test]
    fn load_session_missing_returns_none() {
        let (dd, _tmp) = test_data_dir();
        let result = dd
            .load_session(&PathBuf::from("/work"), &uuid::Uuid::new_v4().to_string())
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_latest_session_missing_returns_none() {
        let (dd, _tmp) = test_data_dir();
        let result = dd
            .load_latest_session(&PathBuf::from("/nonexistent"))
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn save_session_invalid_uuid_errors() {
        let (dd, _tmp) = test_data_dir();
        let mut session = Session::new("valid".to_string(), PathBuf::from("/work"));
        session.id = "not-a-uuid".to_string();
        let result = dd.save_session(&session);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid session UUID")
        );
    }

    #[test]
    fn load_session_invalid_uuid_errors() {
        let (dd, _tmp) = test_data_dir();
        let result = dd.load_session(&PathBuf::from("/work"), "not-a-uuid");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid session UUID")
        );
    }

    #[test]
    fn sessions_dir_structure() {
        let (dd, _tmp) = test_data_dir();
        let sessions_dir = dd.sessions_dir();
        assert_eq!(sessions_dir, dd.data_dir.join("sessions"));
    }

    #[test]
    fn multiple_sessions_same_working_dir() {
        let (dd, _tmp) = test_data_dir();
        let working_dir = PathBuf::from("/work");

        let session1 = Session::new(uuid::Uuid::new_v4().to_string(), working_dir.clone());
        let session2 = Session::new(uuid::Uuid::new_v4().to_string(), working_dir.clone());

        dd.save_session(&session1).unwrap();
        dd.save_session(&session2).unwrap();

        // Both sessions should be loadable
        let loaded1 = dd
            .load_session(&working_dir, &session1.id)
            .unwrap()
            .unwrap();
        let loaded2 = dd
            .load_session(&working_dir, &session2.id)
            .unwrap()
            .unwrap();

        assert_eq!(loaded1.id, session1.id);
        assert_eq!(loaded2.id, session2.id);

        // Latest should be session2 (last saved)
        let latest = dd.load_latest_session(&working_dir).unwrap().unwrap();
        assert_eq!(latest.id, session2.id);
    }

    #[test]
    fn different_working_dirs_isolated() {
        let (dd, _tmp) = test_data_dir();

        let session1 = Session::new(uuid::Uuid::new_v4().to_string(), PathBuf::from("/work1"));
        let session2 = Session::new(uuid::Uuid::new_v4().to_string(), PathBuf::from("/work2"));

        dd.save_session(&session1).unwrap();
        dd.save_session(&session2).unwrap();

        // Each working dir should have its own latest
        let latest1 = dd
            .load_latest_session(&PathBuf::from("/work1"))
            .unwrap()
            .unwrap();
        let latest2 = dd
            .load_latest_session(&PathBuf::from("/work2"))
            .unwrap()
            .unwrap();

        assert_eq!(latest1.id, session1.id);
        assert_eq!(latest2.id, session2.id);
    }
}
