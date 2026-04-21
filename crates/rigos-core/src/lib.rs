//! RigOS Core (The Agent Harness & Reasoning Core)
//!
//! This crate implements Layer 1 and Layer 2 of the Enterprise architecture.
//! It defines the traits for agents and the execution loop (Engine)
//! that orchestrates the perception, reasoning, and action cycle.

pub mod agent;
pub mod engine;
pub mod state;

// Re-export main types for ease of use.
pub use agent::Agent;
pub use engine::AgentEngine;
pub use state::AgentState;
