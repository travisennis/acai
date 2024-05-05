use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

pub fn create_data_dir() {
    let coding_assistant_data_dir = get_data_dir();

    if let Some(p) = coding_assistant_data_dir.parent() {
        fs::create_dir_all(p).expect("Directory not created.");
    };
}

pub fn save_messages<T: Serialize>(messages: &[T]) {
    let coding_assistant_data_dir = get_data_dir();

    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    let in_ms =
        since_the_epoch.as_secs() * 1000 + u64::from(since_the_epoch.subsec_nanos()) / 1_000_000;

    let output_file = format!("{in_ms}.json");
    let output_path = coding_assistant_data_dir.join("history").join(output_file);

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

fn get_data_dir() -> std::path::PathBuf {
    let home_dir = dirs::home_dir().expect("Home dir not found.");
    home_dir.join(".config/coding-assistant")
}
