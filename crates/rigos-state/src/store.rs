use crate::models::AgentStep;
use anyhow::{Context, Result};
use chrono::Utc;
use rigos_core::AgentState;
use sqlx::{PgPool, SqlitePool};
use tracing::{info, instrument, warn};
use uuid::Uuid;

#[async_trait::async_trait]
pub trait TimeTravelStore: Send + Sync {
    async fn save_step(&self, step: AgentStep) -> Result<()>;
    async fn get_trace(&self, trace_id: Uuid) -> Result<Vec<AgentStep>>;
    async fn rewind_to(&self, trace_id: Uuid, step_number: i32) -> Result<AgentState>;
    async fn create_branch(
        &self,
        parent_step_id: Uuid,
        new_payload: serde_json::Value,
    ) -> Result<AgentStep>;
}

#[derive(Clone)]
pub enum StateStore {
    Postgres(PgPool),
    Sqlite(SqlitePool),
}

impl StateStore {
    pub async fn new(db_url: &str) -> std::result::Result<Self, sqlx::Error> {
        if db_url.starts_with("postgres") {
            let pool = PgPool::connect(db_url).await?;
            Ok(StateStore::Postgres(pool))
        } else {
            let pool = SqlitePool::connect(db_url).await?;
            Ok(StateStore::Sqlite(pool))
        }
    }
}

#[async_trait::async_trait]
impl TimeTravelStore for StateStore {
    #[instrument(skip(self, step))]
    async fn save_step(&self, step: AgentStep) -> Result<()> {
        info!(step_id = %step.step_id, trace_id = %step.trace_id, step_number = step.step_number, "TimeTravelStore: saving immutable step");

        match self {
            StateStore::Postgres(pool) => {
                let q = r#"INSERT INTO agent_steps 
                    (step_id, trace_id, parent_step_id, step_number, payload, tool_calls, output, requires_approval)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#;
                sqlx::query(q)
                    .bind(step.step_id)
                    .bind(step.trace_id)
                    .bind(step.parent_step_id)
                    .bind(step.step_number)
                    .bind(&step.payload)
                    .bind(&step.tool_calls)
                    .bind(&step.output)
                    .bind(step.requires_approval)
                    .execute(pool)
                    .await
                    .context("Failed to insert step into Postgres")?;
            }
            StateStore::Sqlite(pool) => {
                let q = r#"INSERT INTO agent_steps 
                    (step_id, trace_id, parent_step_id, step_number, payload, tool_calls, output, requires_approval)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#;
                sqlx::query(q)
                    .bind(step.step_id)
                    .bind(step.trace_id)
                    .bind(step.parent_step_id)
                    .bind(step.step_number)
                    .bind(&step.payload)
                    .bind(&step.tool_calls)
                    .bind(&step.output)
                    .bind(step.requires_approval)
                    .execute(pool)
                    .await
                    .context("Failed to insert step into SQLite")?;
            }
        }
        Ok(())
    }

    #[instrument(skip(self))]
    async fn get_trace(&self, trace_id: Uuid) -> Result<Vec<AgentStep>> {
        let steps = match self {
            StateStore::Postgres(pool) => {
                sqlx::query_as::<_, AgentStep>(
                    r#"SELECT * FROM agent_steps WHERE trace_id = $1 ORDER BY step_number ASC"#,
                )
                .bind(trace_id)
                .fetch_all(pool)
                .await
            }
            StateStore::Sqlite(pool) => {
                sqlx::query_as::<_, AgentStep>(
                    r#"SELECT * FROM agent_steps WHERE trace_id = ?1 ORDER BY step_number ASC"#,
                )
                .bind(trace_id)
                .fetch_all(pool)
                .await
            }
        }?;

        if steps.is_empty() {
            warn!(trace_id = %trace_id, "TimeTravelStore: trace not found");
        }

        Ok(steps)
    }

    #[instrument(skip(self))]
    async fn rewind_to(&self, trace_id: Uuid, step_number: i32) -> Result<AgentState> {
        let step = match self {
            StateStore::Postgres(pool) => {
                sqlx::query_as::<_, AgentStep>(r#"SELECT * FROM agent_steps WHERE trace_id = $1 AND step_number <= $2 ORDER BY step_number DESC LIMIT 1"#)
                .bind(trace_id).bind(step_number)
                .fetch_optional(pool)
                .await?
            }
            StateStore::Sqlite(pool) => {
                sqlx::query_as::<_, AgentStep>(r#"SELECT * FROM agent_steps WHERE trace_id = ?1 AND step_number <= ?2 ORDER BY step_number DESC LIMIT 1"#)
                .bind(trace_id).bind(step_number)
                .fetch_optional(pool)
                .await?
            }
        }
        .context("Step not found for rewind")?;

        // Convert the immutable step into the core-agnostic AgentState for replay.
        serde_json::from_value(step.payload).context("Failed to deserialize AgentState from rewind")
    }

    #[instrument(skip(self, new_payload))]
    async fn create_branch(
        &self,
        parent_step_id: Uuid,
        new_payload: serde_json::Value,
    ) -> Result<AgentStep> {
        let parent = match self {
            StateStore::Postgres(pool) => {
                sqlx::query_as::<_, AgentStep>(r#"SELECT * FROM agent_steps WHERE step_id = $1"#)
                    .bind(parent_step_id)
                    .fetch_one(pool)
                    .await
            }
            StateStore::Sqlite(pool) => {
                sqlx::query_as::<_, AgentStep>(r#"SELECT * FROM agent_steps WHERE step_id = ?1"#)
                    .bind(parent_step_id)
                    .fetch_one(pool)
                    .await
            }
        }?;

        let new_step = AgentStep {
            step_id: Uuid::new_v4(),
            trace_id: parent.trace_id,
            parent_step_id: Some(parent_step_id),
            step_number: parent.step_number + 1,
            payload: new_payload,
            tool_calls: None,
            output: None,
            requires_approval: false,
            created_at: Utc::now(),
        };

        self.save_step(new_step.clone()).await?;

        info!(new_step_id = %new_step.step_id, parent_id = %parent_step_id, "TimeTravelStore: branch created successfully");
        Ok(new_step)
    }
}
