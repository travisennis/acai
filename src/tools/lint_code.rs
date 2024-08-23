use std::{collections::HashMap, io, process::Command};

use serde_json::Value;

use crate::llm_api::{JsonSchema, ToolDefinition};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
}

pub struct LintCode;

impl ToolDefinition for LintCode {
    fn name(&self) -> &'static str {
        "lint_code"
    }
    fn description(&self) -> &'static str {
        "Lints the provided code base using a specified command and returns the results. This function helps identify and report potential issues, style violations, or errors in the code, improving code quality and consistency."
    }
    fn get_parameters(&self) -> JsonSchema {
        let mut properties = HashMap::new();

        properties.insert(
            "instructions".to_owned(),
            JsonSchema::String {
                description: "The reason for the linting call.".to_string(),
            },
        );

        JsonSchema::Object {
            required: properties.keys().map(|s| (*s).clone()).collect(),
            properties,
        }
    }
}

pub fn callable_func() -> Result<Value, Error> {
    let output = Command::new("cargo")
        .args([
            "clippy",
            "--",
            "-Dclippy::all",
            "-Dclippy::pedantic",
            "-Wclippy::unwrap_used",
            "-Wclippy::expect_used",
            "-Wclippy::nursery",
        ])
        .output()?;

    Ok(Value::String(
        String::from_utf8_lossy(&output.stdout).into_owned(),
    ))
}
