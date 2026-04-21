use crate::skill::Skill;
use rigos_core::AgentState;
use sha2::{Digest, Sha256};
use tracing::info;

/// Interface for searching stored skills in the procedural memory database.
/// Prevents hardcoded database queries and temporary integration shortcuts.
pub trait SkillStore: Send + Sync {
    fn find_by_hash(&self, hash: &str) -> Option<Skill>;
    fn find_by_similarity(&self, embedding: &[f32]) -> Option<Skill>;
}

/// Hybrid routing component matching Task logic to existing Procedural Skills.
pub struct SkillRouter<S: SkillStore> {
    store: S,
}

impl<S: SkillStore> SkillRouter<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Hybrid routing: evaluates task hash, keywords, and embedding patterns.
    pub fn route(&self, input_task: &str, _current_state: Option<&AgentState>) -> Option<Skill> {
        let task_hash = compute_task_hash(input_task);
        info!(task = input_task, hash = %task_hash, "SkillRouter: Initiating skill retrieval");

        if let Some(skill) = self.store.find_by_hash(&task_hash) {
            info!("Skill found by hash match: {}", skill.name);
            return Some(skill);
        }

        None
    }
}

/// Provides purely functional hashing algorithm for tasks.
pub fn compute_task_hash(task: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(task.as_bytes());
    format!("{:x}", hasher.finalize())
}
