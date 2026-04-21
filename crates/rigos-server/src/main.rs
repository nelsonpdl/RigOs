use anyhow::Result;
use clap::Parser;
use rigos_mcp::{McpRouter, ToolArtifact, ToolRegistry};
use rigos_sandbox::SandboxEngine;
use rigos_server::{
    config::{Cli, RigOSConfig},
    grpc::grpc_server,
    server::RigOSDaemon,
};
use rigos_state::StateStore;
use std::net::SocketAddr;

use rigos_mcp::remote_opa::RemoteOpaClient;
use rigos_mcp::RuntimeOpaClient;

#[derive(Debug, Clone, Default)]
struct ActiveToolRegistry {
    tools: std::collections::HashMap<String, ToolArtifact>,
}

impl ToolRegistry for ActiveToolRegistry {
    fn resolve_tool(&self, name: &str) -> Result<Option<ToolArtifact>> {
        Ok(self.tools.get(name).cloned())
    }
}

fn build_tool_registry(
    tools: &[rigos_server::config::ToolConfig],
) -> anyhow::Result<ActiveToolRegistry> {
    let mut actual_registry = ActiveToolRegistry::default();
    for active_tool in tools.iter() {
        let bytes = std::fs::read(&active_tool.wasm_path).map_err(|error| {
            anyhow::anyhow!(
                "Failed loading configured WASM tool '{}' from '{}': {}",
                active_tool.name,
                active_tool.wasm_path.display(),
                error
            )
        })?;
        let artifact = ToolArtifact::new(&active_tool.name, bytes);
        tracing::info!(
            tool = %active_tool.name,
            artifact_sha256 = %artifact.sha256,
            "Registered secure tool artifact"
        );
        actual_registry
            .tools
            .insert(active_tool.name.clone(), artifact);
    }
    Ok(actual_registry)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = RigOSConfig::load(&cli)?;

    // Tracing initialization structurally independent
    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    tracing::info!(
        "RigOS Daemon starting with config: {:?}",
        config.server.addr
    );

    let state_store = StateStore::new(&config.database.url).await?;
    let sandbox = SandboxEngine::new()?;

    let opa_client = if let Some(url) = config.opa.url.clone() {
        tracing::info!("Initializing OPAL remote client at {}", url);
        let remote =
            RemoteOpaClient::new(url, config.opa.bearer_token.clone(), config.opa.timeout_ms)?;
        RuntimeOpaClient::Remote(remote)
    } else {
        tracing::info!("No Remote OPAL URL configured. Defaulting explicitly to Deny-All posture.");
        RuntimeOpaClient::DenyAll
    };

    let actual_registry = build_tool_registry(&config.tools)?;

    let mcp_router = McpRouter::new(
        sandbox.clone(),
        state_store.clone(),
        opa_client,
        actual_registry,
    );

    let addr: SocketAddr = config.server.addr.parse()?;
    let daemon = RigOSDaemon::new(state_store.clone(), mcp_router.clone(), addr);

    if config.grpc.enabled {
        let grpc_addr: SocketAddr = config.grpc.addr.parse()?;
        tokio::select! {
            res = daemon.run() => res?,
            res = grpc_server(mcp_router, grpc_addr, config.grpc.tls.clone()) => res?,
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Shutdown signal received - shutting down gracefully");
            }
        }
    } else {
        tokio::select! {
            res = daemon.run() => res?,
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Shutdown signal received - shutting down gracefully");
            }
        }
    }

    tracing::info!("RigOS Daemon shutdown complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rigos_server::config::ToolConfig;
    use std::path::PathBuf;

    #[test]
    fn red_team_missing_wasm_path_aborts_daemon_startup() {
        let tools = vec![ToolConfig {
            name: "ghost_tool".to_string(),
            wasm_path: PathBuf::from("/tmp/non_existent_tool.wasm"),
        }];

        let result = build_tool_registry(&tools);
        assert!(
            result.is_err(),
            "Daemon must abort on unlocatable WASM mappings"
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Failed loading configured WASM tool"));
    }
}
