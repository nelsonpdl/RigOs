use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Represents a single immutable checkpoint in the agent's timeline.
/// This structure is the foundation of Layer 11 (State & Durability) and
/// enables the "Time-Travel Debugger" by persisting this in a database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// Unique identifier for this step (e.g., UUID or state Hash).
    pub step_id: String,

    /// The parent step ID, allowing us to reconstruct the tree (DAG)
    /// in case of branching/time-travel.
    pub parent_step_id: Option<String>,

    /// The current context, LLM messages, or Working Memory (Layer 5.1).
    pub payload: Value,

    /// Defines if the agent requires human intervention (Human-in-the-loop)
    /// before proceeding to the next step. (Layer 8 - Action Guardrails).
    pub requires_human_approval: bool,

    /// Indicates whether the current task is considered finished.
    pub is_finished: bool,
}

impl AgentState {
    /// Creates a clean initial state.
    pub fn new_initial(payload: Value) -> Self {
        Self {
            step_id: uuid::Uuid::new_v4().to_string(),
            parent_step_id: None,
            payload,
            requires_human_approval: false,
            is_finished: false,
        }
    }
}
