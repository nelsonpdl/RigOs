//! Wasmtime Execution Sandbox (Layer 8 Guardrails)
//!
//! Provides an isolated environment for the agent to execute
//! tools compiled to WebAssembly.

pub mod engine;

pub use engine::{SandboxEngine, SandboxLimits};
