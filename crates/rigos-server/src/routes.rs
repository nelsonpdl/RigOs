use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use rigos_mcp::{McpRequest, McpResponse, McpRouter, OpaPolicyClient, ToolRegistry};
use rigos_state::TimeTravelStore;
use serde::{Deserialize, Serialize};
use tracing::{error, info, instrument};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    pub code: String,
}

// Tenant ID extractor for multi-tenant enforcement.
#[derive(Debug)]
pub struct TenantId(pub String);

#[axum::async_trait]
impl<S> axum::extract::FromRequestParts<S> for TenantId
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ApiError>);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        let tenant_id = parts
            .headers
            .get("X-Tenant-ID")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "default".to_string());

        if tenant_id.is_empty() {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    error: "Missing or empty X-Tenant-ID header".into(),
                    code: "MISSING_TENANT".into(),
                }),
            ));
        }

        Ok(TenantId(tenant_id))
    }
}

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    pub trace_id: Option<Uuid>,
    pub mcp_request: McpRequest,
}

#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    pub trace_id: Uuid,
    pub response: McpResponse,
}

pub fn http_routes<S, O, T>(
    mcp_router: McpRouter<S, O, T>,
    _state_store: rigos_state::StateStore,
) -> Router
where
    S: TimeTravelStore + Clone + Send + Sync + 'static,
    O: OpaPolicyClient + Clone + Send + Sync + 'static,
    T: ToolRegistry + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/mcp/dispatch", post(dispatch_mcp::<S, O, T>))
        .route("/agents/execute", post(execute_agent::<S, O, T>))
        .route("/health", get(|| async { "RigOS Daemon OK" }))
        .with_state(mcp_router)
}

#[instrument(skip(mcp_router, tenant))]
async fn dispatch_mcp<S, O, T>(
    State(mcp_router): State<McpRouter<S, O, T>>,
    tenant: TenantId,
    Json(payload): Json<ExecuteRequest>,
) -> std::result::Result<Json<ExecuteResponse>, (StatusCode, Json<ApiError>)>
where
    S: TimeTravelStore + Clone + Send + Sync + 'static,
    O: OpaPolicyClient + Clone + Send + Sync + 'static,
    T: ToolRegistry + Clone + Send + Sync + 'static,
{
    let trace_id = payload.trace_id.unwrap_or_else(Uuid::new_v4);
    let step_parent = Uuid::new_v4();
    let step_number = 1;

    info!(
        tenant_id = %tenant.0,
        trace_id = %trace_id,
        tool = %payload.mcp_request.tool_name,
        "MCP dispatch request received"
    );

    match mcp_router
        .dispatch(payload.mcp_request, trace_id, step_parent, step_number)
        .await
    {
        Ok(response) => Ok(Json(ExecuteResponse { trace_id, response })),
        Err(e) => {
            error!(tenant_id = %tenant.0, trace_id = %trace_id, error = %e, "MCP dispatch failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: e.to_string(),
                    code: "MCP_DISPATCH_ERROR".into(),
                }),
            ))
        }
    }
}

#[instrument(skip(state, tenant))]
async fn execute_agent<S, O, T>(
    state: State<McpRouter<S, O, T>>,
    tenant: TenantId,
    payload: Json<ExecuteRequest>,
) -> std::result::Result<Json<ExecuteResponse>, (StatusCode, Json<ApiError>)>
where
    S: TimeTravelStore + Clone + Send + Sync + 'static,
    O: OpaPolicyClient + Clone + Send + Sync + 'static,
    T: ToolRegistry + Clone + Send + Sync + 'static,
{
    dispatch_mcp(state, tenant, payload).await
}
