use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, anyhow};
use sha2::{Digest, Sha256};

use super::Session;

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

    /// Save a session to `sessions/{dir_hash}/{session.id}.json` with atomic write
    /// and update the `latest` reference.
    pub fn save_session(&self, session: &Session) -> anyhow::Result<PathBuf> {
        uuid::Uuid::parse_str(&session.id).map_err(|e| {
            let id = &session.id;
            anyhow!("Invalid session UUID '{id}': {e}")
        })?;

        let dir_hash = Self::dir_hash(&session.working_dir);
        let session_dir = self.sessions_dir().join(&dir_hash);
        let session_path = session_dir.join(format!("{}.json", session.id));

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
        let target = format!("{session_id}.json");

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
            .join(format!("{session_id}.json"));

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
            .map(|s| s.trim_end_matches(".json").to_string())
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
            .join(format!("{id}.json"));

        if !session_path.exists() {
            return Ok(None);
        }

        Session::load(&session_path).map(Some)
    }
}
