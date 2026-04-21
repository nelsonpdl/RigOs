use crate::routes::http_routes;
use anyhow::Result;
use axum::{serve, Router};
use rigos_mcp::{McpRouter, OpaPolicyClient, ToolRegistry};
use rigos_state::StateStore;
use std::net::SocketAddr;
use tokio::{net::TcpListener, signal};
use tracing::{info, instrument};

pub struct RigOSDaemon<
    O: OpaPolicyClient + Clone + 'static,
    T: ToolRegistry + Clone + Send + Sync + 'static,
> {
    state_store: StateStore,
    mcp_router: McpRouter<StateStore, O, T>,
    addr: SocketAddr,
}

impl<O: OpaPolicyClient + Clone + 'static, T: ToolRegistry + Clone + Send + Sync + 'static>
    RigOSDaemon<O, T>
{
    pub fn new(
        state_store: StateStore,
        mcp_router: McpRouter<StateStore, O, T>,
        addr: SocketAddr,
    ) -> Self {
        Self {
            state_store,
            mcp_router,
            addr,
        }
    }

    #[instrument(skip(self))]
    pub async fn run(self) -> Result<()> {
        info!("RigOS Daemon starting on {}", self.addr);

        // OpenTelemetry initialization (SOC2)
        opentelemetry::global::set_text_map_propagator(
            opentelemetry_sdk::propagation::TraceContextPropagator::new(),
        );

        let app = Router::new()
            .merge(http_routes(
                self.mcp_router.clone(),
                self.state_store.clone(),
            ))
            .layer(tower_http::trace::TraceLayer::new_for_http());

        let listener = TcpListener::bind(&self.addr).await?;
        let http_server = serve(listener, app);

        tokio::select! {
            res = http_server => { res? },
            _ = signal::ctrl_c() => { info!("Shutdown signal received"); }
        }

        info!("RigOS Daemon shutdown gracefully");
        Ok(())
    }
}
