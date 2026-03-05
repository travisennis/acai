use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context};
use sha2::{Digest, Sha256};

use std::sync::OnceLock;

use super::Session;

pub static DATA_DIR_INSTANCE: OnceLock<DataDir> = OnceLock::new();

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

    /// Returns the global singleton instance.
    ///
    /// # Panics
    /// Panics if the global instance has not been initialized.
    #[allow(clippy::expect_used)]
    pub fn global() -> &'static Self {
        DATA_DIR_INSTANCE
            .get()
            .expect("Data dir is not initialized")
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
    /// and update the `latest` symlink.
    pub fn save_session(&self, session: &Session) -> anyhow::Result<PathBuf> {
        uuid::Uuid::parse_str(&session.id)
            .map_err(|e| {
                let id = &session.id;
                anyhow!("Invalid session UUID '{id}': {e}")
            })?;

        let dir_hash = Self::dir_hash(&session.working_dir);
        let session_dir = self.sessions_dir().join(&dir_hash);
        let session_path = session_dir.join(format!("{}.json", session.id));

        session.save(&session_path)?;

        // Update latest symlink atomically
        let latest_link = session_dir.join("latest");
        let temp_link = session_dir.join(".latest_tmp");
        let target = format!("{}.json", session.id);

        // Remove temp symlink if it exists
        let _ = fs::remove_file(&temp_link);
        std::os::unix::fs::symlink(&target, &temp_link)
            .with_context(|| format!("Failed to create temp symlink at {}", temp_link.display()))?;
        fs::rename(&temp_link, &latest_link)
            .with_context(|| format!("Failed to rename symlink to {}", latest_link.display()))?;

        Ok(session_path)
    }

    /// Load the most recent session for a given working directory.
    pub fn load_latest_session(&self, working_dir: &Path) -> anyhow::Result<Option<Session>> {
        let dir_hash = Self::dir_hash(working_dir);
        let latest_link = self.sessions_dir().join(&dir_hash).join("latest");

        if !latest_link.exists() {
            return Ok(None);
        }

        let target = fs::read_link(&latest_link)
            .with_context(|| format!("Failed to read symlink: {}", latest_link.display()))?;

        let session_path = self.sessions_dir().join(&dir_hash).join(target);
        if !session_path.exists() {
            return Ok(None);
        }

        Session::load(&session_path).map(Some)
    }

    /// Load a specific session by UUID, scoped to a working directory.
    pub fn load_session(&self, working_dir: &Path, id: &str) -> anyhow::Result<Option<Session>> {
        uuid::Uuid::parse_str(id)
            .map_err(|e| anyhow!("Invalid session UUID '{id}': {e}"))?;

        let dir_hash = Self::dir_hash(working_dir);
        let session_path = self.sessions_dir().join(&dir_hash).join(format!("{id}.json"));

        if !session_path.exists() {
            return Ok(None);
        }

        Session::load(&session_path).map(Some)
    }
}
