use crate::opal::OpaPolicyClient;
use crate::protocol::{McpRequest, McpResponse};
use crate::registry::ToolRegistry;
use anyhow::{Context, Result};
use chrono::Utc;
use rigos_sandbox::{SandboxEngine, SandboxLimits};
use rigos_state::{AgentStep, TimeTravelStore};
use tracing::{instrument, warn};
use uuid::Uuid;

#[derive(Clone)]
pub struct McpRouter<S: TimeTravelStore, O: OpaPolicyClient, T: ToolRegistry> {
    sandbox: SandboxEngine,
    state_store: S,
    opa_client: O,
    tool_registry: T,
}

impl<S: TimeTravelStore, O: OpaPolicyClient, T: ToolRegistry> McpRouter<S, O, T> {
    pub fn new(sandbox: SandboxEngine, state_store: S, opa_client: O, tool_registry: T) -> Self {
        Self {
            sandbox,
            state_store,
            opa_client,
            tool_registry,
        }
    }

    #[instrument(skip(self, req))]
    pub async fn dispatch(
        &self,
        req: McpRequest,
        trace_id: Uuid,
        parent_step_id: Uuid,
        step_number: i32,
    ) -> Result<McpResponse> {
        let input = serde_json::to_value(&req).context("Failed to serialize MCP request")?;

        let artifact = match self.tool_registry.resolve_tool(&req.tool_name)? {
            Some(artifact) => artifact,
            None => {
                warn!(tool = %req.tool_name, "MCP dispatch rejected unknown tool");
                return Ok(McpResponse {
                    success: false,
                    output: serde_json::json!({}),
                    error_msg: Some(format!(
                        "Unknown or unauthorized internal tool: {}",
                        req.tool_name
                    )),
                });
            }
        };

        if artifact.name != req.tool_name {
            warn!(
                requested_tool = %req.tool_name,
                artifact_tool = %artifact.name,
                "MCP dispatch rejected mismatched registry artifact"
            );
            return Ok(McpResponse {
                success: false,
                output: serde_json::json!({}),
                error_msg: Some("Tool registry returned a mismatched artifact".to_string()),
            });
        }

        let policy_decision = match self.opa_client.evaluate("mcp/tool/allow", &input).await {
            Ok(decision) => decision,
            Err(error) => {
                warn!(
                    tool = %req.tool_name,
                    error = %error,
                    "MCP dispatch failed closed after OPA evaluation error"
                );
                return Ok(McpResponse {
                    success: false,
                    output: serde_json::json!({}),
                    error_msg: Some(format!("OPAL policy evaluation failed closed: {}", error)),
                });
            }
        };

        if !policy_decision.allowed {
            return Ok(McpResponse {
                success: false,
                output: serde_json::json!({}),
                error_msg: Some(format!("OPAL RBAC denied: {}", policy_decision.reason)),
            });
        }

        let output = self
            .sandbox
            .execute(
                &artifact.wasm_bytes,
                SandboxLimits {
                    max_fuel: 5_000_000,
                    max_memory_bytes: 64 * 1024 * 1024,
                },
            )
            .await?;

        let step = AgentStep {
            step_id: Uuid::new_v4(),
            trace_id,
            parent_step_id: Some(parent_step_id),
            step_number,
            payload: input,
            tool_calls: Some(serde_json::json!({
                "tool_name": req.tool_name,
                "artifact_sha256": artifact.sha256,
                "policy_path": "mcp/tool/allow",
                "policy_allowed": policy_decision.allowed,
                "policy_reason": policy_decision.reason,
            })),
            output: Some(serde_json::json!({ "result": output })),
            requires_approval: false,
            created_at: Utc::now(),
        };

        self.state_store.save_step(step).await?;

        Ok(McpResponse {
            success: true,
            output: serde_json::json!({ "result": output }),
            error_msg: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opal::PolicyDecision;
    use crate::registry::ToolArtifact;
    use anyhow::{anyhow, Result};
    use rigos_core::AgentState;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct RecordingStore {
        steps: Arc<Mutex<Vec<AgentStep>>>,
    }

    impl RecordingStore {
        fn step_count(&self) -> usize {
            self.steps.lock().map(|steps| steps.len()).unwrap_or(0)
        }
    }

    #[async_trait::async_trait]
    impl TimeTravelStore for RecordingStore {
        async fn save_step(&self, step: AgentStep) -> Result<()> {
            let mut steps = self
                .steps
                .lock()
                .map_err(|_| anyhow!("recording store lock poisoned"))?;
            steps.push(step);
            Ok(())
        }

        async fn get_trace(&self, _trace_id: Uuid) -> Result<Vec<AgentStep>> {
            self.steps
                .lock()
                .map(|steps| steps.clone())
                .map_err(|_| anyhow!("recording store lock poisoned"))
        }

        async fn rewind_to(&self, _trace_id: Uuid, _step_number: i32) -> Result<AgentState> {
            Err(anyhow!("rewind is not used by MCP router tests"))
        }

        async fn create_branch(
            &self,
            _parent_step_id: Uuid,
            _new_payload: serde_json::Value,
        ) -> Result<AgentStep> {
            Err(anyhow!("branching is not used by MCP router tests"))
        }
    }

    #[derive(Clone)]
    struct StaticPolicyClient {
        decision: PolicyDecision,
    }

    #[async_trait::async_trait]
    impl OpaPolicyClient for StaticPolicyClient {
        async fn evaluate(
            &self,
            _policy_path: &str,
            _input: &serde_json::Value,
        ) -> Result<PolicyDecision> {
            Ok(self.decision.clone())
        }
    }

    #[derive(Clone)]
    struct ErrorPolicyClient;

    #[async_trait::async_trait]
    impl OpaPolicyClient for ErrorPolicyClient {
        async fn evaluate(
            &self,
            _policy_path: &str,
            _input: &serde_json::Value,
        ) -> Result<PolicyDecision> {
            Err(anyhow!("policy transport unavailable"))
        }
    }

    #[derive(Clone)]
    struct StaticToolRegistry {
        artifact: ToolArtifact,
    }

    impl ToolRegistry for StaticToolRegistry {
        fn resolve_tool(&self, name: &str) -> Result<Option<ToolArtifact>> {
            if name == self.artifact.name {
                Ok(Some(self.artifact.clone()))
            } else {
                Ok(None)
            }
        }
    }

    fn registry() -> StaticToolRegistry {
        StaticToolRegistry {
            artifact: ToolArtifact::new("dangerous.tool", b"\0asm\x01\0\0\0".to_vec()),
        }
    }

    fn request(tool_name: &str) -> McpRequest {
        McpRequest {
            tool_name: tool_name.to_string(),
            payload: serde_json::json!({ "operation": "delete_database" }),
        }
    }

    #[tokio::test]
    async fn denied_policy_does_not_persist_agent_step() -> Result<()> {
        let store = RecordingStore::default();
        let router = McpRouter::new(
            SandboxEngine::new()?,
            store.clone(),
            StaticPolicyClient {
                decision: PolicyDecision {
                    allowed: false,
                    reason: "red-team denial".to_string(),
                },
            },
            registry(),
        );

        let response = router
            .dispatch(request("dangerous.tool"), Uuid::new_v4(), Uuid::new_v4(), 1)
            .await?;

        assert!(!response.success);
        assert_eq!(store.step_count(), 0);
        assert!(response
            .error_msg
            .as_deref()
            .is_some_and(|error| error.contains("OPAL RBAC denied")));
        Ok(())
    }

    #[tokio::test]
    async fn red_team_unknown_tool_must_not_execute_even_when_policy_allows() -> Result<()> {
        let store = RecordingStore::default();
        let router = McpRouter::new(
            SandboxEngine::new()?,
            store,
            StaticPolicyClient {
                decision: PolicyDecision {
                    allowed: true,
                    reason: "allowed by policy".to_string(),
                },
            },
            registry(),
        );

        let response = router
            .dispatch(
                request("unregistered.destructive.tool"),
                Uuid::new_v4(),
                Uuid::new_v4(),
                1,
            )
            .await?;

        assert!(
            !response.success,
            "unknown tools must fail closed instead of executing unregistered artifacts"
        );
        Ok(())
    }

    #[tokio::test]
    async fn allowed_tool_persists_artifact_and_policy_evidence() -> Result<()> {
        let store = RecordingStore::default();
        let router = McpRouter::new(
            SandboxEngine::new()?,
            store.clone(),
            StaticPolicyClient {
                decision: PolicyDecision {
                    allowed: true,
                    reason: "allowed by policy".to_string(),
                },
            },
            registry(),
        );

        let response = router
            .dispatch(request("dangerous.tool"), Uuid::new_v4(), Uuid::new_v4(), 1)
            .await?;

        assert!(response.success);
        let steps = store.get_trace(Uuid::new_v4()).await?;
        assert_eq!(steps.len(), 1);
        let tool_calls = steps
            .first()
            .and_then(|step| step.tool_calls.as_ref())
            .ok_or_else(|| anyhow!("missing tool call audit evidence"))?;
        assert_eq!(tool_calls["tool_name"], "dangerous.tool");
        assert!(tool_calls["artifact_sha256"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64));
        assert_eq!(tool_calls["policy_allowed"], true);
        Ok(())
    }

    #[tokio::test]
    async fn known_tool_with_opa_error_fails_closed_without_persisting_step() -> Result<()> {
        let store = RecordingStore::default();
        let router = McpRouter::new(
            SandboxEngine::new()?,
            store.clone(),
            ErrorPolicyClient,
            registry(),
        );

        let response = router
            .dispatch(request("dangerous.tool"), Uuid::new_v4(), Uuid::new_v4(), 1)
            .await?;

        assert!(!response.success);
        assert_eq!(store.step_count(), 0);
        assert!(response
            .error_msg
            .as_deref()
            .is_some_and(|error| error.contains("failed closed")));
        Ok(())
    }
}
