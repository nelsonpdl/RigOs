use crate::opal::{OpaPolicyClient, PolicyDecision};
use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RemoteOpaClient {
    transport: Arc<dyn OpaTransport>,
    url: String,
    bearer_token: Option<String>,
}

#[derive(Debug, Clone)]
struct OpaHttpResponse {
    status: u16,
    body: String,
}

#[async_trait::async_trait]
trait OpaTransport: Send + Sync + std::fmt::Debug {
    async fn post_json(
        &self,
        url: &str,
        bearer_token: Option<&str>,
        body: &serde_json::Value,
    ) -> Result<OpaHttpResponse>;
}

#[derive(Debug)]
struct ReqwestOpaTransport {
    client: Client,
}

#[async_trait::async_trait]
impl OpaTransport for ReqwestOpaTransport {
    async fn post_json(
        &self,
        url: &str,
        bearer_token: Option<&str>,
        body: &serde_json::Value,
    ) -> Result<OpaHttpResponse> {
        let mut request = self.client.post(url).json(body);
        if let Some(token) = bearer_token {
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .await
            .map_err(|error| anyhow!("Network error requesting OPAL engine: {}", error))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .context("Failed to read OPAL response body")?;

        Ok(OpaHttpResponse { status, body })
    }
}

#[derive(Serialize)]
struct OpaRequest {
    policy_path: String,
    input: serde_json::Value,
}

#[derive(Deserialize)]
struct OpaResponse {
    result: Option<PolicyDecisionRaw>,
    allowed: Option<bool>,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct PolicyDecisionRaw {
    allowed: bool,
    reason: String,
}

impl RemoteOpaClient {
    pub fn new(url: String, bearer_token: Option<String>, timeout_ms: u64) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()?;

        Ok(Self {
            transport: Arc::new(ReqwestOpaTransport { client }),
            url,
            bearer_token,
        })
    }

    #[cfg(test)]
    fn with_transport(
        url: String,
        bearer_token: Option<String>,
        transport: Arc<dyn OpaTransport>,
    ) -> Self {
        Self {
            transport,
            url,
            bearer_token,
        }
    }

    fn parse_decision(response: OpaHttpResponse) -> Result<PolicyDecision> {
        if !(200..300).contains(&response.status) {
            return Err(anyhow!(
                "OPAL denied request implicitly via non-2xx bound: {}",
                response.status
            ));
        }

        let result_json: OpaResponse = serde_json::from_str(&response.body)
            .context("OPAL returned malformed JSON payload boundaries")?;

        let (allowed, reason) = if let Some(result) = result_json.result {
            (result.allowed, result.reason)
        } else if let (Some(allowed), Some(reason)) = (result_json.allowed, result_json.reason) {
            (allowed, reason)
        } else {
            return Err(anyhow!(
                "OPAL response missing explicit constraint validation paths"
            ));
        };

        Ok(PolicyDecision { allowed, reason })
    }
}

#[async_trait::async_trait]
impl OpaPolicyClient for RemoteOpaClient {
    async fn evaluate(
        &self,
        policy_path: &str,
        input: &serde_json::Value,
    ) -> Result<PolicyDecision> {
        let request_body = serde_json::to_value(OpaRequest {
            policy_path: policy_path.to_string(),
            input: input.clone(),
        })
        .context("Failed to serialize OPAL request")?;

        let response = self
            .transport
            .post_json(&self.url, self.bearer_token.as_deref(), &request_body)
            .await?;

        Self::parse_decision(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Debug)]
    struct StaticTransport {
        response: Result<OpaHttpResponse, String>,
    }

    #[async_trait::async_trait]
    impl OpaTransport for StaticTransport {
        async fn post_json(
            &self,
            _url: &str,
            _bearer_token: Option<&str>,
            _body: &serde_json::Value,
        ) -> Result<OpaHttpResponse> {
            self.response.clone().map_err(|error| anyhow!(error))
        }
    }

    fn client_with_response(response: Result<OpaHttpResponse, String>) -> RemoteOpaClient {
        RemoteOpaClient::with_transport(
            "http://opa.test/v1/data/mcp/tool/allow".to_string(),
            None,
            Arc::new(StaticTransport { response }),
        )
    }

    fn response(status: u16, body: serde_json::Value) -> OpaHttpResponse {
        OpaHttpResponse {
            status,
            body: body.to_string(),
        }
    }

    #[tokio::test]
    async fn remote_opa_allows_explicit_true() -> Result<()> {
        let client = client_with_response(Ok(response(
            200,
            json!({
                "result": { "allowed": true, "reason": "Explicit OPAL logic trigger" }
            }),
        )));

        let decision = client.evaluate("mcp/tool/allow", &json!({})).await?;
        assert!(decision.allowed);
        Ok(())
    }

    #[tokio::test]
    async fn remote_opa_denies_explicit_false() -> Result<()> {
        let client = client_with_response(Ok(response(
            200,
            json!({
                "result": { "allowed": false, "reason": "Red team violation" }
            }),
        )));

        let decision = client.evaluate("mcp/tool/allow", &json!({})).await?;
        assert!(!decision.allowed);
        Ok(())
    }

    #[tokio::test]
    async fn remote_opa_fails_closed_on_timeout() -> Result<()> {
        let client = client_with_response(Err(
            "Network error requesting OPAL engine: timeout".to_string()
        ));

        let decision = client.evaluate("mcp/tool/allow", &json!({})).await;
        assert!(decision.is_err());
        assert!(decision
            .err()
            .is_some_and(|error| error.to_string().contains("Network error")));
        Ok(())
    }

    #[tokio::test]
    async fn remote_opa_fails_closed_on_non_2xx() -> Result<()> {
        let client = client_with_response(Ok(OpaHttpResponse {
            status: 500,
            body: "{}".to_string(),
        }));

        let decision = client.evaluate("mcp/tool/allow", &json!({})).await;
        assert!(decision.is_err());
        assert!(decision
            .err()
            .is_some_and(|error| error.to_string().contains("implicitly via non-2xx bound")));
        Ok(())
    }

    #[tokio::test]
    async fn remote_opa_fails_closed_on_malformed_json() -> Result<()> {
        let client = client_with_response(Ok(OpaHttpResponse {
            status: 200,
            body: "bad json payload".to_string(),
        }));

        let decision = client.evaluate("mcp/tool/allow", &json!({})).await;
        assert!(decision.is_err());
        assert!(decision.err().is_some_and(|error| error
            .to_string()
            .contains("malformed JSON payload boundaries")));
        Ok(())
    }

    #[tokio::test]
    async fn remote_opa_fails_closed_on_missing_allowed_field() -> Result<()> {
        let client = client_with_response(Ok(response(
            200,
            json!({
                "result": { "reason": "Missing allowed boolean entirely" }
            }),
        )));

        let decision = client.evaluate("mcp/tool/allow", &json!({})).await;
        assert!(decision.is_err());
        assert!(decision.err().is_some_and(|error| error
            .to_string()
            .contains("malformed JSON payload boundaries")));
        Ok(())
    }
}
