use crate::config::GrpcTlsConfig;
use anyhow::Context;
use anyhow::Result;
use rigos_mcp::{McpRequest, McpRouter, OpaPolicyClient, ToolRegistry};
use rigos_state::TimeTravelStore;
use tonic::{
    transport::{Identity, Server, ServerTlsConfig},
    Request, Response, Status,
};
use tracing::{info, instrument, warn};
use uuid::Uuid;

pub mod rigos_agent {
    tonic::include_proto!("rigos.agent.v1");
}

use rigos_agent::rig_os_agent_server::{RigOsAgent, RigOsAgentServer};
use rigos_agent::{DispatchMcpRequest, DispatchMcpResponse, HealthRequest, HealthResponse};

pub struct GrpcAgentService<
    S: TimeTravelStore + Clone + Send + Sync + 'static,
    O: OpaPolicyClient + Clone + Send + Sync + 'static,
    T: ToolRegistry + Clone + Send + Sync + 'static,
> {
    mcp_router: McpRouter<S, O, T>,
}

impl<
        S: TimeTravelStore + Clone + Send + Sync + 'static,
        O: OpaPolicyClient + Clone + Send + Sync + 'static,
        T: ToolRegistry + Clone + Send + Sync + 'static,
    > GrpcAgentService<S, O, T>
{
    pub fn new(mcp_router: McpRouter<S, O, T>) -> Self {
        Self { mcp_router }
    }
}

#[tonic::async_trait]
impl<
        S: TimeTravelStore + Clone + Send + Sync + 'static,
        O: OpaPolicyClient + Clone + Send + Sync + 'static,
        T: ToolRegistry + Clone + Send + Sync + 'static,
    > RigOsAgent for GrpcAgentService<S, O, T>
{
    #[instrument(skip(self))]
    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        Ok(Response::new(HealthResponse {
            status: "OK".to_string(),
        }))
    }

    #[instrument(skip(self, request))]
    async fn dispatch_mcp(
        &self,
        request: Request<DispatchMcpRequest>,
    ) -> Result<Response<DispatchMcpResponse>, Status> {
        let req = request.into_inner();

        let trace_id = if req.trace_id.is_empty() {
            Uuid::new_v4()
        } else {
            Uuid::parse_str(&req.trace_id).map_err(|_| {
                warn!(trace_id = %req.trace_id, "gRPC rejected invalid trace ID");
                Status::invalid_argument("Invalid trace_id")
            })?
        };
        let step_parent = Uuid::new_v4();
        let step_number = 1;

        let payload_json = serde_json::from_str(&req.payload_json).map_err(|e| {
            warn!("gRPC failed parsing payload JSON: {}", e);
            Status::invalid_argument("Malformed payload_json bounds")
        })?;

        let mcp_request = McpRequest {
            tool_name: req.tool_name,
            payload: payload_json,
        };

        info!(trace_id = %trace_id, tool = %mcp_request.tool_name, "gRPC dispatch request received natively");

        match self
            .mcp_router
            .dispatch(mcp_request, trace_id, step_parent, step_number)
            .await
        {
            Ok(res) => Ok(Response::new(DispatchMcpResponse {
                trace_id: trace_id.to_string(),
                success: res.success,
                output_json: serde_json::to_string(&res.output).map_err(|error| {
                    warn!(trace_id = %trace_id, error = %error, "gRPC failed serializing MCP output");
                    Status::internal("Failed to serialize MCP output")
                })?,
                error_msg: res.error_msg.map_or_else(String::new, std::convert::identity),
            })),
            Err(e) => {
                warn!(trace_id = %trace_id, error = %e, "gRPC dispatch execution trapped");
                Err(Status::internal(format!("Internal Daemon error: {}", e)))
            }
        }
    }
}

pub async fn grpc_server<S, O, T>(
    mcp_router: McpRouter<S, O, T>,
    addr: std::net::SocketAddr,
    tls_config: Option<GrpcTlsConfig>,
) -> Result<()>
where
    S: TimeTravelStore + Clone + Send + Sync + 'static,
    O: OpaPolicyClient + Clone + Send + Sync + 'static,
    T: ToolRegistry + Clone + Send + Sync + 'static,
{
    let service = GrpcAgentService::new(mcp_router);

    let mut server = Server::builder();
    if let Some(tls_config) = tls_config {
        let cert = std::fs::read(&tls_config.cert_path).with_context(|| {
            format!(
                "Failed to read gRPC TLS certificate: {}",
                tls_config.cert_path.display()
            )
        })?;
        let key = std::fs::read(&tls_config.key_path).with_context(|| {
            format!(
                "Failed to read gRPC TLS private key: {}",
                tls_config.key_path.display()
            )
        })?;
        let identity = Identity::from_pem(cert, key);
        server = server.tls_config(ServerTlsConfig::new().identity(identity))?;

        info!(
            "gRPC server fully bounded with TLS and listening defensively on {}",
            addr
        );
    } else {
        info!(
            "gRPC server fully bounded and listening defensively on {}",
            addr
        );
    }

    server
        .add_service(RigOsAgentServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use rigos_core::AgentState;
    use rigos_mcp::PolicyDecision;
    use rigos_sandbox::SandboxEngine;
    use rigos_state::{AgentStep, TimeTravelStore};

    #[derive(Clone, Default)]
    struct TestStore;
    #[async_trait::async_trait]
    impl TimeTravelStore for TestStore {
        async fn save_step(&self, _step: AgentStep) -> Result<()> {
            Ok(())
        }

        async fn get_trace(&self, _trace_id: Uuid) -> Result<Vec<AgentStep>> {
            Ok(vec![])
        }

        async fn rewind_to(&self, _trace_id: Uuid, _step_number: i32) -> Result<AgentState> {
            Err(anyhow!("rewind is not used by gRPC tests"))
        }

        async fn create_branch(&self, _parent: Uuid, _pay: serde_json::Value) -> Result<AgentStep> {
            Err(anyhow!("branching is not used by gRPC tests"))
        }
    }

    #[derive(Clone)]
    struct TestOpa;
    #[async_trait::async_trait]
    impl OpaPolicyClient for TestOpa {
        async fn evaluate(&self, _path: &str, _in: &serde_json::Value) -> Result<PolicyDecision> {
            Ok(PolicyDecision {
                allowed: false,
                reason: "deny".to_string(),
            })
        }
    }

    #[derive(Clone)]
    struct TestRegistry;
    impl ToolRegistry for TestRegistry {
        fn resolve_tool(&self, _name: &str) -> Result<Option<rigos_mcp::ToolArtifact>> {
            Ok(None)
        }
    }

    fn test_service() -> Result<GrpcAgentService<TestStore, TestOpa, TestRegistry>> {
        let router = McpRouter::new(SandboxEngine::new()?, TestStore, TestOpa, TestRegistry);
        Ok(GrpcAgentService::new(router))
    }

    #[tokio::test]
    async fn grpc_health_returns_ok() -> Result<()> {
        let srv = test_service()?;
        let res = srv
            .health(Request::new(HealthRequest {}))
            .await?
            .into_inner();
        assert_eq!(res.status, "OK");
        Ok(())
    }

    #[tokio::test]
    async fn grpc_invalid_json_returns_structured_error() -> Result<()> {
        let srv = test_service()?;
        let req = Request::new(DispatchMcpRequest {
            tenant_id: "t1".to_string(),
            trace_id: Uuid::new_v4().to_string(),
            tool_name: "test".to_string(),
            payload_json: "{ bad json ".to_string(),
        });
        let error = match srv.dispatch_mcp(req).await {
            Ok(_) => return Err(anyhow!("expected malformed JSON to fail")),
            Err(error) => error,
        };
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(error.message().contains("Malformed payload_json"));
        Ok(())
    }

    #[tokio::test]
    async fn grpc_unknown_tool_fails_closed_via_router() -> Result<()> {
        let srv = test_service()?;
        let req = Request::new(DispatchMcpRequest {
            tenant_id: "t1".to_string(),
            trace_id: Uuid::new_v4().to_string(),
            tool_name: "ghost_tool".to_string(),
            payload_json: "{}".to_string(),
        });
        let res = srv.dispatch_mcp(req).await?.into_inner();
        assert!(!res.success);
        assert!(res.error_msg.contains("Unknown"));
        Ok(())
    }

    #[tokio::test]
    async fn grpc_empty_trace_id_generates_new_trace_id() -> Result<()> {
        let srv = test_service()?;
        let req = Request::new(DispatchMcpRequest {
            tenant_id: "t1".to_string(),
            trace_id: String::new(),
            tool_name: "ghost_tool".to_string(),
            payload_json: "{}".to_string(),
        });
        let res = srv.dispatch_mcp(req).await?.into_inner();
        assert!(Uuid::parse_str(&res.trace_id).is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn grpc_invalid_trace_id_is_rejected() -> Result<()> {
        let srv = test_service()?;
        let req = Request::new(DispatchMcpRequest {
            tenant_id: "t1".to_string(),
            trace_id: "invalid-uuid-string".to_string(),
            tool_name: "ghost_tool".to_string(),
            payload_json: "{}".to_string(),
        });
        let error = match srv.dispatch_mcp(req).await {
            Ok(_) => return Err(anyhow!("expected invalid trace ID to fail")),
            Err(error) => error,
        };
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(error.message().contains("Invalid trace_id"));
        Ok(())
    }
}
