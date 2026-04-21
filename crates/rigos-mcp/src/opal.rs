use anyhow::Result;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct PolicyDecision {
    pub allowed: bool,
    pub reason: String,
}

#[async_trait::async_trait]
pub trait OpaPolicyClient: Send + Sync {
    async fn evaluate(&self, policy_path: &str, input: &Value) -> Result<PolicyDecision>;
}

#[derive(Clone)]
pub enum RuntimeOpaClient {
    Remote(crate::remote_opa::RemoteOpaClient),
    DenyAll,
}

#[async_trait::async_trait]
impl OpaPolicyClient for RuntimeOpaClient {
    async fn evaluate(&self, policy_path: &str, input: &Value) -> Result<PolicyDecision> {
        match self {
            Self::Remote(client) => client.evaluate(policy_path, input).await,
            Self::DenyAll => Ok(PolicyDecision {
                allowed: false,
                reason:
                    "Strict security mode: OPAL integration required for tool executions natively"
                        .to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn missing_opa_url_defaults_to_deny_all() -> Result<()> {
        let client = RuntimeOpaClient::DenyAll;
        let res = client.evaluate("mcp/tool/allow", &json!({})).await?;
        assert!(!res.allowed, "Default posture MUST deny implicitly");
        assert!(
            res.reason.contains("Strict security mode"),
            "Must emit strict security reason"
        );
        Ok(())
    }
}
