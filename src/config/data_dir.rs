use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::anyhow;
use log::error;
use serde::Serialize;

use once_cell::sync::OnceCell;

pub static DATA_DIR_INSTANCE: OnceCell<DataDir> = OnceCell::new();

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

    pub fn save_messages<T: Serialize>(&self, messages: &[T]) -> Option<PathBuf> {
        let Ok(in_ms) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            error!("Time went backwards");
            return None;
        };
        let in_ms = in_ms.as_millis();

        let output_path = self.data_dir.join("history").join(format!("{in_ms}.json"));

        if let Some(p) = output_path.parent() {
            if let Err(e) = fs::create_dir_all(p) {
                error!("Failed to create directory: {e}");
                return None;
            }
        }

        match serde_json::to_string_pretty(&messages) {
            Ok(json_string) => {
                if let Err(e) = std::fs::write(output_path.clone(), json_string) {
                    error!("Failed to write to file: {e}");
                    None
                } else {
                    Some(output_path)
                }
            }
            Err(e) => {
                error!("Failed to serialize messages: {e}");
                None
            }
        }
    }
}
