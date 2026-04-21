use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

/// The central entity defining a learned Procedural Memory (Skill).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Unique identifier for the skill.
    pub id: Uuid,
    /// Human-readable name, e.g., "construction_permit_miami_v1"
    pub name: String,
    /// Semantic versioning, e.g., "1.2.3"
    pub version: String,
    /// Brief generated summary of the skill's purpose.
    pub description: String,
    /// Timestamp of creation.
    pub created_at: DateTime<Utc>,
    /// Reference to the original execution DAG (AgentState) that generated this skill.
    pub source_trace_id: Uuid,
    /// Defines how and when this skill gets activated by the SkillRouter.
    pub trigger_pattern: TriggerPattern,
    /// The actual procedural "manual" or steps.
    pub procedure: Procedure,
    /// Real-world usage metadata (success rate, invocations).
    pub metadata: SkillMetadata,
    /// Cryptographic checksum to verify integrity and prevent tampering.
    pub checksum: String,
}

/// Determines the heuristic or embedding vector used to fetch the skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerPattern {
    /// Optional embedding vector for similarity search.
    pub embedding: Option<Vec<f32>>,
    /// Hash of the original task request.
    pub task_hash: String,
    /// Keyword heuristics for rapid static routing.
    pub keywords: Vec<String>,
    /// The prompt string that originally spawned this procedural pattern.
    pub example_prompt: String,
}

/// The actual sequence of instructions, guarded by safety rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Procedure {
    /// Sequence of logical actions.
    pub steps: Vec<Step>,
    /// Array of Model Context Protocol (MCP) tool names required.
    pub required_tools: Vec<String>,
    /// Human-in-the-loop and constraint guards.
    pub guardrails: Vec<Guardrail>,
    /// Analytics: Expected tokens saved by not thinking from scratch.
    pub estimated_tokens_saved: u32,
}

/// A deterministic action step inside the procedure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub id: usize,
    /// Description or exact tool call invocation.
    pub action: String,
    /// Pre-computed or templated JSON parameters for the tool.
    pub parameters: Value,
    /// Type assertion for the expected return schema.
    pub expected_output_type: String,
}

/// A safety checkpoint that dictates what happens if an anomaly occurs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guardrail {
    /// Evaluation rule (e.g., "amount > 0").
    pub condition: String,
    /// Action upon failure (e.g., "human_approval", "abort", "fallback").
    pub action_if_fail: String,
}

/// Governance statistics for the skill lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Percentage of successful runs using this skill.
    pub success_rate: f32,
    /// Total number of invocations in production.
    pub usage_count: u32,
    /// Timestamp of the last time this skill was triggered.
    pub last_used: Option<DateTime<Utc>>,
    /// Multi-tenant isolation boundary (which organization owns this).
    pub owner_tenant: Option<String>,
}
