use std::{
    fs,
    path::{Path, PathBuf},
};

use glob::Pattern;
use ignore::{Walk, WalkBuilder};
use log::error;
use serde_json::{json, Value};
use termtree::Tree;

pub struct FileInfo {
    pub path: PathBuf,
    pub content: String,
}

pub fn get_file_info(
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
    let canonical_path = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(e) => {
            error!("Failed to canonicalize path: {}", e);
            return false;
        }
    };
    let path_str = canonical_path.to_str().unwrap();

    let included = include_patterns
        .iter()
        .any(|pattern| Pattern::new(pattern).unwrap().matches(path_str));
    let excluded = exclude_patterns
        .iter()
        .any(|pattern| Pattern::new(pattern).unwrap().matches(path_str));

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

pub fn get_file_tree(
    dir: &Path,
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> std::io::Result<String> {
    let canonical_root_path = dir.canonicalize()?;
    let parent_directory = label(&canonical_root_path);
    let tree = WalkBuilder::new(&canonical_root_path)
        .git_ignore(true)
        .build()
        .filter_map(std::result::Result::ok)
        .fold(Tree::new(parent_directory), |mut root, entry| {
            let path = entry.path();
            if let Ok(relative_path) = path.strip_prefix(&canonical_root_path) {
                let mut current_tree = &mut root;
                for component in relative_path.components() {
                    let component_str = component.as_os_str().to_string_lossy().to_string();

                    // Check if the current component should be excluded from the tree
                    if !should_include_file(path, include_patterns, exclude_patterns) {
                        break;
                    }

                    current_tree = if let Some(pos) = current_tree
                        .leaves
                        .iter_mut()
                        .position(|child| child.root == component_str)
                    {
                        &mut current_tree.leaves[pos]
                    } else {
                        let new_tree = Tree::new(component_str.clone());
                        current_tree.leaves.push(new_tree);
                        current_tree.leaves.last_mut().unwrap()
                    };
                }
            }
            root
        });

    Ok(tree.to_string())
}

fn label<P: AsRef<Path>>(p: P) -> String {
    let path = p.as_ref();
    if path.file_name().is_none() {
        let current_dir = std::env::current_dir().unwrap();
        current_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(".")
            .to_owned()
    } else {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_owned()
    }
}
