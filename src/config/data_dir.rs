use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

pub struct DataDir {
    data_dir: std::path::PathBuf,
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
    pub fn new() -> Self {
        let home_dir = dirs::home_dir().expect("Home dir not found.");
        let data_dir = home_dir.join(".cache/coding-assistant");

        if !data_dir.exists() {
            fs::create_dir_all(&data_dir).expect("Failed to create data directory");
        }

        Self { data_dir }
    }

    pub fn save_messages<T: Serialize>(&self, messages: &[T]) {
        let in_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();

        let output_path = self.data_dir.join("history").join(format!("{in_ms}.json"));

        if let Some(p) = output_path.parent() {
            fs::create_dir_all(p).expect("Directory not created.");
        }

        match serde_json::to_string_pretty(&messages) {
            Ok(json_string) => {
                if let Err(e) = std::fs::write(output_path, json_string) {
                    eprintln!("Failed to write to file: {e}");
                }
            }
            Err(e) => eprintln!("Failed to serialize messages: {e}"),
        }
    }
}
