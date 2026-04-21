use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "RigOS Daemon Server")]
pub struct Cli {
    /// Path to config file (YAML)
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Override server address
    #[arg(short, long, default_value = "0.0.0.0:8080")]
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    pub id: String,
    pub max_concurrent_sessions: u32,
    pub max_tokens_per_session: u64,
    pub sandbox_fuel_limit: u64,
    pub sandbox_memory_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelConfig {
    pub endpoint: String,
    pub service_name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    pub name: String,
    pub wasm_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpaConfig {
    pub url: Option<String>,
    pub bearer_token: Option<String>,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    pub addr: String,
    pub enabled: bool,
    pub tls: Option<GrpcTlsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcTlsConfig {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigOSConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub otel: OtelConfig,
    pub tenants: Vec<TenantConfig>,
    pub log_level: String,
    pub sandbox: SandboxGlobalConfig,
    pub grpc: GrpcConfig,
    pub opa: OpaConfig,
    #[serde(default)]
    pub tools: Vec<ToolConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub addr: String,
    pub graceful_shutdown_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxGlobalConfig {
    pub default_fuel: u64,
    pub default_memory_bytes: usize,
}

impl RigOSConfig {
    pub fn load(cli: &Cli) -> Result<Self> {
        let default_config = RigOSConfig::default();

        let mut config = if let Some(path) = &cli.config {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read config file: {}", path.display()))?;
            serde_yaml::from_str(&content).context("Failed to parse YAML config")?
        } else {
            default_config
        };

        // CLI overrides
        if cli.addr != "0.0.0.0:8080" {
            config.server.addr = cli.addr.clone();
        }

        // Environment variable overrides (highest priority)
        if let Ok(addr) = std::env::var("RIGOS_SERVER_ADDR") {
            config.server.addr = addr;
        }
        if let Ok(otel) = std::env::var("RIGOS_OTEL_ENDPOINT") {
            config.otel.endpoint = otel;
        }
        if let Ok(log) = std::env::var("RIGOS_LOG_LEVEL") {
            config.log_level = log;
        }
        if let Ok(opa_url) = std::env::var("RIGOS_OPA_URL") {
            config.opa.url = Some(opa_url);
        }
        if let Ok(token) = std::env::var("RIGOS_OPA_BEARER_TOKEN") {
            config.opa.bearer_token = Some(token);
        }
        if let Ok(timeout_ms) = std::env::var("RIGOS_OPA_TIMEOUT_MS") {
            config.opa.timeout_ms = timeout_ms
                .parse()
                .context("Failed to parse RIGOS_OPA_TIMEOUT_MS")?;
        }
        if let Ok(grpc_addr) = std::env::var("RIGOS_GRPC_ADDR") {
            config.grpc.addr = grpc_addr;
        }
        if let Ok(grpc_enabled) = std::env::var("RIGOS_GRPC_ENABLED") {
            config.grpc.enabled = grpc_enabled == "1" || grpc_enabled.eq_ignore_ascii_case("true");
        }
        let grpc_tls_cert_path = std::env::var("RIGOS_GRPC_TLS_CERT_PATH").ok();
        let grpc_tls_key_path = std::env::var("RIGOS_GRPC_TLS_KEY_PATH").ok();
        match (grpc_tls_cert_path, grpc_tls_key_path) {
            (Some(cert_path), Some(key_path)) => {
                config.grpc.tls = Some(GrpcTlsConfig {
                    cert_path: PathBuf::from(cert_path),
                    key_path: PathBuf::from(key_path),
                });
            }
            (Some(_), None) | (None, Some(_)) => {
                anyhow::bail!(
                    "RIGOS_GRPC_TLS_CERT_PATH and RIGOS_GRPC_TLS_KEY_PATH must be configured together"
                );
            }
            (None, None) => {}
        }

        Ok(config)
    }
}

impl Default for RigOSConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                addr: "0.0.0.0:8080".to_string(),
                graceful_shutdown_seconds: 30,
            },
            database: DatabaseConfig {
                url: "sqlite://rigos.db".to_string(),
                max_connections: 32,
            },
            otel: OtelConfig {
                endpoint: "http://localhost:4317".to_string(),
                service_name: "rigos-daemon".to_string(),
                enabled: true,
            },
            tenants: vec![TenantConfig {
                id: "default".to_string(),
                max_concurrent_sessions: 100,
                max_tokens_per_session: 1_000_000,
                sandbox_fuel_limit: 10_000_000,
                sandbox_memory_bytes: 128 * 1024 * 1024,
            }],
            log_level: "info".to_string(),
            sandbox: SandboxGlobalConfig {
                default_fuel: 5_000_000,
                default_memory_bytes: 64 * 1024 * 1024,
            },
            grpc: GrpcConfig {
                addr: "0.0.0.0:50051".to_string(),
                enabled: true,
                tls: None,
            },
            opa: OpaConfig {
                url: None,
                bearer_token: None,
                timeout_ms: 1500,
            },
            tools: vec![],
        }
    }
}
