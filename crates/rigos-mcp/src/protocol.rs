use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The standardized invocation request for an MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRequest {
    pub tool_name: String,
    pub payload: Value,
}

/// The result returned by a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResponse {
    pub success: bool,
    pub output: Value,
    pub error_msg: Option<String>,
}
