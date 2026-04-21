use anyhow::{Context, Result};
use tracing::{info, instrument};
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

#[derive(Clone)]
pub struct SandboxEngine {
    engine: Engine,
    linker: Linker<SandboxState>,
}

pub struct SandboxState {
    limits: StoreLimits,
}

impl SandboxEngine {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.wasm_backtrace(true);
        config.wasm_component_model(true);
        config.async_support(true);

        let engine = Engine::new(&config)?;
        let linker = Linker::new(&engine);
        // WASI is intentionally not linked until policy grants explicit host capabilities.

        Ok(Self { engine, linker })
    }

    #[instrument(skip(self, wasm_bytes))]
    pub async fn execute(&self, wasm_bytes: &[u8], limits: SandboxLimits) -> Result<String> {
        let module =
            Module::new(&self.engine, wasm_bytes).context("Failed to compile WASM module")?;

        let state = SandboxState {
            limits: StoreLimitsBuilder::new()
                .memory_size(limits.max_memory_bytes)
                .build(),
        };

        let mut store = Store::new(&self.engine, state);
        store.set_fuel(limits.max_fuel)?;
        store.limiter(|state| &mut state.limits);

        let _instance = self.linker.instantiate_async(&mut store, &module).await?;

        info!("SandboxEngine: tool executed in WASM isolated environment with secured limits");
        Ok("Tool executed safely".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct SandboxLimits {
    pub max_fuel: u64,
    pub max_memory_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn red_team_loop_triggers_fuel_trap() -> Result<()> {
        let engine = SandboxEngine::new()?;
        let wasm = wat::parse_str(
            r#"
        (module
            (func $run (loop br 0))
            (start $run)
        )
        "#,
        )?;

        let limits = SandboxLimits {
            max_fuel: 10_000,
            max_memory_bytes: 1024 * 1024,
        };
        let result = engine.execute(&wasm, limits).await;

        assert!(result.is_err());
        let err_msg = match result {
            Ok(_) => String::new(),
            Err(error) => format!("{error:?}"),
        };
        assert!(err_msg.contains("fuel"), "Expected fuel, got: {}", err_msg);
        Ok(())
    }

    #[tokio::test]
    async fn red_team_memory_exhaustion_trap() -> Result<()> {
        let engine = SandboxEngine::new()?;
        let wasm = wat::parse_str(
            r#"
        (module
            (memory 100)
        )
        "#,
        )?;

        let limits = SandboxLimits {
            max_fuel: 1_000_000,
            max_memory_bytes: 65_536,
        };
        let result = engine.execute(&wasm, limits).await;

        assert!(result.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn red_team_invalid_wasm_bytes_trap() -> Result<()> {
        let engine = SandboxEngine::new()?;
        let wasm = b"invalid formatting strings";

        let limits = SandboxLimits {
            max_fuel: 100_000,
            max_memory_bytes: 1024 * 1024,
        };
        let result = engine.execute(wasm, limits).await;

        assert!(result.is_err());
        Ok(())
    }
}
