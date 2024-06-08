use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

pub struct DataDir {
    data_dir: std::path::PathBuf,
}

impl DataDir {
    pub fn new() -> Self {
        let home_dir = dirs::home_dir().expect("Home dir not found.");
        let data_dir = home_dir.join(".cache/coding-assistant");

        Self { data_dir }
    }

    pub fn create() -> Self {
        let home_dir = dirs::home_dir().expect("Home dir not found.");
        let data_dir = home_dir.join(".cache/coding-assistant");

        if let Some(p) = data_dir.parent() {
            fs::create_dir_all(p).expect("Directory not created.");
        };

        Self { data_dir }
    }

    pub fn save_messages<T: Serialize>(&self, messages: &[T]) {
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");

        let in_ms = since_the_epoch.as_secs() * 1000
            + u64::from(since_the_epoch.subsec_nanos()) / 1_000_000;

        let output_file = format!("{in_ms}.json");
        let output_path = self.data_dir.join("history").join(output_file);

        if let Some(p) = output_path.parent() {
            fs::create_dir_all(p).expect("Directory not created.");
        };

        // Save the JSON structure into the other file.
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
