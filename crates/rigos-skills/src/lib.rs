//! Skill Engine and Procedural Memory Layer.
//!
//! This crate corresponds to Layer 5 of the CoALA Enterprise Architecture.
//! It defines the `.skill` object and the algorithms to extract and route
//! procedural memory across agent sessions.

pub mod engine;
pub mod router;
pub mod skill;

pub use engine::SkillEngine;
pub use router::SkillRouter;
pub use skill::{Procedure, Skill, TriggerPattern};
