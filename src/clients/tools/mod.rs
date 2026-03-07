use serde::Serialize;

// =============================================================================
// Module Declarations
// =============================================================================

mod bash;

// =============================================================================
// Tool Types
// =============================================================================

/// Tool definition sent in API requests
#[derive(Serialize, Clone, Debug)]
pub struct Tool {
    #[serde(rename = "type")]
    pub(super) type_: String,
    pub(super) name: String,
    pub(super) description: String,
    pub(super) parameters: serde_json::Value,
}

/// Result of executing a tool
#[derive(Debug)]
pub struct ToolResult {
    pub output: String,
}

// =============================================================================
// Tool Execution
// =============================================================================

/// Execute a tool call
pub(super) async fn execute_tool(name: &str, arguments: &str) -> Result<ToolResult, String> {
    match name {
        "Bash" => Box::pin(bash::execute_bash(arguments)).await,
        _ => Err(format!("Unknown tool: {name}")),
    }
}

// =============================================================================
// Tool Definitions (Re-exports)
// =============================================================================

/// Returns the Bash tool definition
pub fn bash_tool() -> Tool {
    bash::bash_tool()
}
