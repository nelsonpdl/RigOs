use anyhow::Result;
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

#[derive(Parser)]
#[command(author, version, about = "RigOS Command Line Interface")]
struct Cli {
    /// URL of the RigOS Daemon Server
    #[arg(short, long, default_value = "http://127.0.0.1:8080")]
    endpoint: String,

    /// Tenant ID assigned to this operation boundary
    #[arg(short, long, default_value = "default")]
    tenant: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ping the daemon server evaluating TCP health bounds.
    Health,
    /// Dispatches a tool execution task to the MCP router dynamically.
    Dispatch {
        /// Name of the required tool footprint mapped in the registry
        tool_name: String,
        /// JSON-formatted argument matching the tool schema
        payload: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::builder().timeout(Duration::from_secs(60)).build()?;

    match &cli.command {
        Commands::Health => {
            let url = format!("{}/health", cli.endpoint);
            println!("Pinging RigOS Daemon: {}", url);

            let res = client.get(&url).send().await?;
            if res.status().is_success() {
                let text = res.text().await?;
                println!("Daemon health check passed: {}", text);
            } else {
                eprintln!("Daemon health check failed: {:?}", res.status());
            }
        }
        Commands::Dispatch { tool_name, payload } => {
            let url = format!("{}/mcp/dispatch", cli.endpoint);
            let payload_json: Value = serde_json::from_str(payload)
                .map_err(|e| anyhow::anyhow!("Invalid json payload: {}", e))?;

            let execute_req = serde_json::json!({
                "trace_id": uuid::Uuid::new_v4(),
                "mcp_request": {
                    "tool_name": tool_name.clone(),
                    "payload": payload_json
                }
            });

            println!("Dispatching Tool: {} to {}", tool_name, url);
            let res = client
                .post(&url)
                .header("X-Tenant-ID", cli.tenant.clone())
                .json(&execute_req)
                .send()
                .await?;

            if res.status().is_success() {
                let text = res.text().await?;
                println!("Dispatch succeeded:\n{}", text);
            } else {
                let text = res.text().await?;
                eprintln!("Dispatch failed:\n{}", text);
            }
        }
    }

    Ok(())
}
