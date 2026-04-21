use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Trace {
    pub trace_id: Uuid,
    pub session_id: Uuid,
    pub tenant_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub root_prompt: String,
    pub final_status: String,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AgentStep {
    pub step_id: Uuid, // UUID v7 (time-ordered)
    pub trace_id: Uuid,
    pub parent_step_id: Option<Uuid>,
    pub step_number: i32,
    pub payload: serde_json::Value,
    pub tool_calls: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub requires_approval: bool,
    pub created_at: DateTime<Utc>,
}
