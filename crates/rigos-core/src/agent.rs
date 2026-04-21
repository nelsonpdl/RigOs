use crate::state::AgentState;

/// The primary abstraction for Agent logic within RigOS.
/// This matches Layer 1 (Reasoning Core). It allows defining different types of
/// orchestrators (ReAct, Plan-and-Execute, Tree-of-Thoughts) by implementing this trait.
pub trait Agent {
    /// The name or identifier of the agent pattern (e.g., "evaluator", "planner").
    fn name(&self) -> &str;

    /// Executes a single reasoning step.
    /// Takes the current immutable state and returns the next transitioned state.
    fn step(
        &self,
        state: AgentState,
    ) -> impl std::future::Future<Output = anyhow::Result<AgentState>> + Send;
}
