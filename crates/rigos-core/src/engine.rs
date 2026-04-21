use crate::agent::Agent;
use crate::state::AgentState;
use tracing::{info, warn};

/// The internal Engine or Orchestrator (Agent Harness - Layer 2).
/// It is the loop responsible for executing the agent step by step safely.
pub struct AgentEngine<A: Agent + Send + Sync> {
    agent: A,
}

impl<A: Agent + Send + Sync> AgentEngine<A> {
    /// Initializes a new orchestrator by mounting the given Agent.
    pub fn new(agent: A) -> Self {
        Self { agent }
    }

    /// Runs the agent continuously until it finishes or is halted
    /// by a Human-in-the-loop intervention from the Action Guardrails.
    pub async fn run(
        &self,
        initial_state: AgentState,
        execution_budget: u32,
    ) -> anyhow::Result<AgentState> {
        info!("Starting AgentEngine for agent: {}", self.agent.name());
        let mut current_state = initial_state;
        let mut iterations = 0;

        loop {
            if iterations >= execution_budget {
                anyhow::bail!("Agent executed maximum allowed budget ({}) without finishing. Possible infinite loop detected.", execution_budget);
            }
            iterations += 1;

            // Checkpoint Guardrail (HITL): If the state requires human permission, halt the loop.
            if current_state.requires_human_approval {
                warn!(
                    "Execution paused! Human approval required at step_id: {}",
                    current_state.step_id
                );
                // The process suspends and yields control (the server could send a WebHook here).
                break;
            }

            // This is where the agent uses the LLM or Tools (MCP) for a single iteration.
            let next_state = self.agent.step(current_state.clone()).await?;

            // Termination check: if the task is done, exit the loop.
            if next_state.is_finished {
                info!("Agent loop successfully terminated.");
                current_state = next_state;
                break;
            }

            current_state = next_state;
        }

        Ok(current_state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[derive(Clone)]
    struct CountingAgent {
        calls: Arc<AtomicUsize>,
        finish_after_step: bool,
    }

    impl Agent for CountingAgent {
        fn name(&self) -> &str {
            "counting-agent"
        }

        fn step(
            &self,
            mut state: AgentState,
        ) -> impl std::future::Future<Output = Result<AgentState>> + Send {
            let calls = Arc::clone(&self.calls);
            let finish_after_step = self.finish_after_step;

            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                state.is_finished = finish_after_step;
                tokio::task::yield_now().await;
                Ok(state)
            }
        }
    }

    #[tokio::test]
    async fn human_approval_state_halts_before_agent_step() {
        let calls = Arc::new(AtomicUsize::new(0));
        let engine = AgentEngine::new(CountingAgent {
            calls: Arc::clone(&calls),
            finish_after_step: true,
        });
        let mut state = AgentState::new_initial(json!({ "task": "requires approval" }));
        state.requires_human_approval = true;

        let result = engine.run(state, 50).await;

        assert!(result.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn finished_state_exits_after_single_agent_step() {
        let calls = Arc::new(AtomicUsize::new(0));
        let engine = AgentEngine::new(CountingAgent {
            calls: Arc::clone(&calls),
            finish_after_step: true,
        });

        let result = engine
            .run(AgentState::new_initial(json!({ "task": "finish" })), 50)
            .await;

        assert!(result.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(matches!(result, Ok(state) if state.is_finished));
    }

    #[tokio::test]
    async fn red_team_non_terminating_agent_is_guarded_by_iteration_budget() {
        let calls = Arc::new(AtomicUsize::new(0));
        let engine = AgentEngine::new(CountingAgent {
            calls,
            finish_after_step: false,
        });

        let result = engine
            .run(
                AgentState::new_initial(json!({ "task": "never finishes" })),
                50,
            )
            .await;

        assert!(
            result.is_err(),
            "the harness should exit protecting the host from looping"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Possible infinite loop detected"));
    }
}
