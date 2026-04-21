use anyhow::Result;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub struct ToolArtifact {
    pub name: String,
    pub wasm_bytes: Vec<u8>,
    pub sha256: String,
}

impl ToolArtifact {
    pub fn new(name: impl Into<String>, wasm_bytes: Vec<u8>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(&wasm_bytes);
        Self {
            name: name.into(),
            wasm_bytes,
            sha256: format!("{:x}", hasher.finalize()),
        }
    }
}

pub trait ToolRegistry: Send + Sync {
    fn resolve_tool(&self, name: &str) -> Result<Option<ToolArtifact>>;
}
