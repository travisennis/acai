use std::collections::HashMap;

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type")]
pub enum JsonSchema {
    String {
        description: String,
    },
    Object {
        properties: HashMap<String, JsonSchema>,
        required: Vec<String>,
    },
}
