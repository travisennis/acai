use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use anyhow::{Context, anyhow};
use serde::Deserialize;

use crate::config::Session;

/// Represents an AGENTS.md file with its path and content.
///
/// AGENTS.md files contain instructions for the AI agent about project-specific
/// context and behavior. They are loaded from user-level (`~/.cake/AGENTS.md`),
/// XDG config (`~/.config/AGENTS.md`), and project-level (`./AGENTS.md`) locations.
#[derive(Debug, Clone)]
pub struct AgentsFile {
    /// Display path (e.g., "~/.cake/AGENTS.md" or "./AGENTS.md")
    pub path: String,
    /// Content of the file
    pub content: String,
}

/// Manages the data directory for session storage.
///
/// The cache directory defaults to `~/.cache/cake/` and contains cache data,
/// logs, and other ephemeral state. Session files are stored separately at
/// `~/.local/share/cake/sessions/` for durability and discoverability.
///
/// The directories can be overridden by setting the `CAKE_DATA_DIR` environment
/// variable. This is useful for testing and for running cake inside cake
/// (nested invocations) without filesystem collisions.
#[derive(Debug, Clone)]
pub struct DataDir {
    /// The path to the cache/data directory.
    data_dir: PathBuf,
    /// The path to the sessions directory.
    sessions_dir: PathBuf,
}

impl DataDir {
    /// Creates a new data directory instance for session storage.
    ///
    /// If `CAKE_DATA_DIR` is set, uses that path for both cache and sessions.
    /// Otherwise, cache defaults to `~/.cache/cake/` and sessions to
    /// `~/.local/share/cake/sessions/`. Directories are created if they do not exist.
    ///
    /// # Examples
    ///
    /// ```
    /// use cake::config::DataDir;
    /// let data_dir = DataDir::new()?;
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the home directory cannot be determined (when
    /// `CAKE_DATA_DIR` is not set), or if directories cannot be created.
    pub fn new() -> anyhow::Result<Self> {
        let (data_dir, sessions_dir) = if let Ok(custom) = std::env::var("CAKE_DATA_DIR") {
            let custom = PathBuf::from(custom);
            (custom.clone(), custom.join("sessions"))
        } else {
            let home_dir = dirs::home_dir();
            if let Some(home) = home_dir {
                (
                    home.join(".cache").join("cake"),
                    home.join(".local")
                        .join("share")
                        .join("cake")
                        .join("sessions"),
                )
            } else {
                return Err(anyhow!("Could not create data directory."));
            }
        };

        if !data_dir.exists() {
            fs::create_dir_all(&data_dir)?;
        }
        if !sessions_dir.exists() {
            fs::create_dir_all(&sessions_dir)?;
        }

        Ok(Self {
            data_dir,
            sessions_dir,
        })
    }

    /// Returns the path to the cache directory.
    ///
    /// The cache directory is typically `~/.cache/cake/`.
    ///
    /// # Examples
    ///
    /// ```
    /// use cake::config::DataDir;
    /// let data_dir = DataDir::new()?;
    /// let cache_path = data_dir.get_cache_dir();
    /// ```
    pub fn get_cache_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    /// Returns the sessions directory path.
    ///
    /// Defaults to `~/.local/share/cake/sessions/` or `{CAKE_DATA_DIR}/sessions`.
    fn sessions_dir(&self) -> PathBuf {
        self.sessions_dir.clone()
    }

    /// Saves a session to disk with atomic write.
    ///
    /// The session is saved to `~/.local/share/cake/sessions/{session_id}.jsonl`.
    /// The most recent session is determined by file modification time (no symlink needed).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use cake::config::{DataDir, Session};
    /// use std::path::PathBuf;
    ///
    /// let data_dir = DataDir::new()?;
    /// let session = Session::new(uuid::Uuid::new_v4(), PathBuf::from("/project"));
    /// data_dir.save_session(&session)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the session file cannot be written.
    pub fn save_session(&self, session: &Session) -> anyhow::Result<PathBuf> {
        let session_path = self.sessions_dir().join(format!("{}.jsonl", session.id));

        tracing::info!(target: "cake", "Saving session {} to {}", session.id, session_path.display());

        session.save(&session_path)?;

        Ok(session_path)
    }

    /// Loads the most recent session for a given working directory.
    ///
    /// Scans all session files and finds the newest `.jsonl` file by modification
    /// time whose `working_directory` header field matches the given directory.
    /// Returns `None` if no matching sessions exist.
    ///
    /// # Examples
    ///
    /// ```
    /// use cake::config::DataDir;
    /// use std::path::PathBuf;
    ///
    /// let data_dir = DataDir::new()?;
    /// let session = data_dir.load_latest_session(&PathBuf::from("/project"))?;
    /// match session {
    ///     Some(s) => println!("Found session: {}", s.id),
    ///     None => println!("No previous session found"),
    /// }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the session directory cannot be read or if a
    /// matching session file cannot be loaded.
    pub fn load_latest_session(&self, working_dir: &Path) -> anyhow::Result<Option<Session>> {
        let session_dir = self.sessions_dir();

        tracing::info!(target: "cake", "Looking for latest session in {} for {}", session_dir.display(), working_dir.display());

        if !session_dir.exists() {
            tracing::info!(target: "cake", "Session directory does not exist: {}", session_dir.display());
            return Ok(None);
        }

        let result = fs::read_dir(&session_dir)
            .with_context(|| {
                format!(
                    "Failed to read session directory: {}",
                    session_dir.display()
                )
            })?
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
            .filter_map(|entry| {
                let path = entry.path();
                let modified = entry.metadata().ok()?.modified().ok()?;
                let header = read_session_header(&path).ok()?;
                (header.working_directory == working_dir).then_some((path, modified))
            })
            .max_by_key(|(_, modified)| *modified)
            .map(|(path, _)| Session::load(&path))
            .transpose()?;

        if let Some(ref session) = result {
            tracing::info!(target: "cake", "Found latest session: {}", session.id);
        } else {
            tracing::info!(target: "cake", "No session found for working directory");
        }

        Ok(result)
    }

    /// Loads a specific session by UUID.
    ///
    /// Returns the session with the given ID, or `None` if no such session exists.
    ///
    /// # Examples
    ///
    /// ```
    /// use cake::config::DataDir;
    ///
    /// let data_dir = DataDir::new()?;
    /// let session = data_dir.load_session(
    ///     "550e8400-e29b-41d4-a716-446655440000"
    /// )?;
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the session ID is not a valid UUID, or if the
    /// session file exists but cannot be loaded.
    pub fn load_session(&self, id: &str) -> anyhow::Result<Option<Session>> {
        uuid::Uuid::parse_str(id).map_err(|e| anyhow!("Invalid session UUID '{id}': {e}"))?;

        let session_path = self.sessions_dir().join(format!("{id}.jsonl"));

        tracing::info!(target: "cake", "Loading session {id} from {}", session_path.display());

        if !session_path.exists() {
            tracing::info!(target: "cake", "Session file does not exist: {}", session_path.display());
            return Ok(None);
        }

        Session::load(&session_path).map(Some)
    }

    /// Reads AGENTS.md files from user-level and project-level locations.
    ///
    /// Returns a list of found AGENTS.md files with their paths and content.
    /// Files that don't exist are silently skipped.
    ///
    /// The search order is:
    /// 1. User-level: `~/.cake/AGENTS.md`
    /// 2. XDG config: `~/.config/AGENTS.md`
    /// 3. Project-level: `./AGENTS.md`
    ///
    /// # Examples
    ///
    /// ```
    /// use cake::config::DataDir;
    /// use std::path::PathBuf;
    ///
    /// let data_dir = DataDir::new()?;
    /// let agents_files = data_dir.read_agents_files(&PathBuf::from("/project"));
    /// for file in &agents_files {
    ///     println!("Found AGENTS.md at: {}", file.path);
    /// }
    /// ```
    pub fn read_agents_files(&self, working_dir: &Path) -> Vec<AgentsFile> {
        let mut files = Vec::new();

        // User-level AGENTS.md: ~/.cake/AGENTS.md
        let user_agents_path = self.data_dir.parent()
            .and_then(|p| p.parent()) // ~/.cache/cake -> ~/.cache -> ~
            .map(|p| p.join(".cake").join("AGENTS.md"))
            .or_else(|| dirs::home_dir().map(|h| h.join(".cake").join("AGENTS.md")));

        if let Some(ref path) = user_agents_path
            && let Ok(content) = fs::read_to_string(path)
        {
            files.push(AgentsFile {
                path: "~/.cake/AGENTS.md".to_string(),
                content,
            });
        }

        // XDG config AGENTS.md: ~/.config/AGENTS.md
        let xdg_agents_path = dirs::home_dir().map(|h| h.join(".config").join("AGENTS.md"));

        if let Some(ref path) = xdg_agents_path
            && let Ok(content) = fs::read_to_string(path)
        {
            files.push(AgentsFile {
                path: "~/.config/AGENTS.md".to_string(),
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

/// Check whether a string looks like a UUID (8-4-4-4-12 hex format).
/// Used to distinguish `--resume <uuid>` from `--resume <path>`.
pub fn looks_like_uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok()
}

/// Minimal header struct for quickly inspecting a session file without
/// loading the entire conversation history. Supports both v2 and v3 formats.
#[derive(Deserialize)]
struct SessionFileHeader {
    #[allow(dead_code)]
    session_id: String,
    working_directory: PathBuf,
}

/// Reads only the first line of a session file to extract its header.
/// Works with both v2 (`session_start`) and v3 (init) formats.
fn read_session_header(path: &Path) -> anyhow::Result<SessionFileHeader> {
    let file = fs::File::open(path)
        .with_context(|| format!("Failed to open session file: {}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).with_context(|| {
        format!(
            "Failed to read header from session file: {}",
            path.display()
        )
    })?;
    let header: SessionFileHeader = serde_json::from_str(first_line.trim())
        .with_context(|| format!("Failed to parse session header: {}", path.display()))?;
    Ok(header)
}

/// Loads a session from an arbitrary file path.
///
/// This is used by `--resume <path>` and `--fork <path>` to load sessions
/// from files outside the sessions directory (e.g. redirected stream-json output).
///
/// # Errors
///
/// Returns an error if the file cannot be loaded or parsed.
pub fn load_session_from_path(path: &Path) -> anyhow::Result<Session> {
    Session::load(path)
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
            sessions_dir: tmp.path().join("sessions"),
        };
        (dd, tmp)
    }

    #[test]
    fn save_and_load_session_round_trip() {
        let (dd, _tmp) = test_data_dir();
        let session = Session::new(uuid::Uuid::new_v4(), PathBuf::from("/work"));
        dd.save_session(&session).unwrap();
        let loaded = dd.load_session(&session.id.to_string()).unwrap().unwrap();
        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.working_dir, session.working_dir);
    }

    #[test]
    fn save_and_load_latest_session() {
        let (dd, _tmp) = test_data_dir();
        let session = Session::new(uuid::Uuid::new_v4(), PathBuf::from("/work"));
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
        let result = dd.load_session(&uuid::Uuid::new_v4().to_string()).unwrap();
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
    fn load_session_invalid_uuid_errors() {
        let (dd, _tmp) = test_data_dir();
        let result = dd.load_session("not-a-uuid");
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
        let (dd, tmp) = test_data_dir();
        let sessions_dir = dd.sessions_dir();
        assert_eq!(sessions_dir, tmp.path().join("sessions"));
    }

    #[test]
    fn multiple_sessions_same_working_dir() {
        let (dd, _tmp) = test_data_dir();
        let working_dir = PathBuf::from("/work");

        let session1 = Session::new(uuid::Uuid::new_v4(), working_dir.clone());
        let session2 = Session::new(uuid::Uuid::new_v4(), working_dir.clone());

        dd.save_session(&session1).unwrap();
        dd.save_session(&session2).unwrap();

        // Both sessions should be loadable
        let loaded1 = dd.load_session(&session1.id.to_string()).unwrap().unwrap();
        let loaded2 = dd.load_session(&session2.id.to_string()).unwrap().unwrap();

        assert_eq!(loaded1.id, session1.id);
        assert_eq!(loaded2.id, session2.id);

        // Latest should be session2 (last saved)
        let latest = dd.load_latest_session(&working_dir).unwrap().unwrap();
        assert_eq!(latest.id, session2.id);
    }

    #[test]
    fn different_working_dirs_isolated() {
        let (dd, _tmp) = test_data_dir();

        let session1 = Session::new(uuid::Uuid::new_v4(), PathBuf::from("/work1"));
        let session2 = Session::new(uuid::Uuid::new_v4(), PathBuf::from("/work2"));

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

    #[test]
    fn new_respects_cake_data_dir_env() {
        let tmp = TempDir::new().unwrap();
        let custom_path = tmp.path().join("custom_cake");

        // SAFETY: test is single-threaded for this env var; no other test
        // reads CAKE_DATA_DIR concurrently.
        unsafe {
            std::env::set_var("CAKE_DATA_DIR", &custom_path);
        }
        let dd = DataDir::new().unwrap();
        unsafe {
            std::env::remove_var("CAKE_DATA_DIR");
        }

        assert_eq!(dd.data_dir, custom_path);
        assert!(custom_path.exists());
    }
}
