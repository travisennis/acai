use std::{
    fs,
    path::{Path, PathBuf},
};

use glob::Pattern;
use ignore::Walk;
use log::error;
use serde_json::{json, Value};

pub struct FileInfo {
    pub path: PathBuf,
    pub content: String,
}

pub fn get_files(
    path: &Path,
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> anyhow::Result<Vec<FileInfo>> {
    let mut files = Vec::new();

    for result in Walk::new(path) {
        // Each item yielded by the iterator is either a directory entry or an
        // error, so either print the path or the error.
        match result {
            Ok(entry) => {
                let path = entry.path();
                if path.is_file() && should_include_file(path, include_patterns, exclude_patterns) {
                    let content = read_file_contents(path)?;

                    files.push(FileInfo {
                        path: path.to_path_buf(),
                        content,
                    });
                }
            }
            Err(err) => error!("ERROR: {err}"),
        }
    }
    Ok(files)
}

pub fn get_content_blocks(files: &[FileInfo]) -> Vec<Value> {
    let mut blocks = Vec::new();
    for file in files {
        blocks.push(json!({
            "path": file.path.display().to_string(),
            "content": file.content,
        }));
    }
    blocks
}

pub fn parse_patterns(patterns: &Option<String>) -> Vec<String> {
    match patterns {
        Some(patterns) if !patterns.is_empty() => {
            patterns.split(',').map(|s| s.trim().to_string()).collect()
        }
        _ => vec![],
    }
}

pub fn should_include_file(
    path: &Path,
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> bool {
    // ~~~ Clean path ~~~
    let canonical_path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(e) => {
            error!("Failed to canonicalize path: {}", e);
            return false;
        }
    };
    let path_str = canonical_path.to_str().unwrap();

    // ~~~ Check glob patterns ~~~
    let included = include_patterns
        .iter()
        .any(|pattern| Pattern::new(pattern).unwrap().matches(path_str));
    let excluded = exclude_patterns
        .iter()
        .any(|pattern| Pattern::new(pattern).unwrap().matches(path_str));

    // ~~~ Decision ~~~
    match (included, excluded) {
        (true, _) => true,      // If include pattern match, include the file
        (false, true) => false, // If the path is excluded, exclude it
        (false, false) => include_patterns.is_empty(), // If no include patterns are provided, include everything
    }
}

pub fn read_file_contents<P: AsRef<Path>>(path: P) -> anyhow::Result<String> {
    let contents = fs::read_to_string(path)?;
    Ok(contents)
}

pub fn extension_to_name(extension: &str) -> &'static str {
    match extension {
        "ts" => "typescript",
        "py" => "python",
        "rs" => "rust",
        _ => "unknown",
    }
}
