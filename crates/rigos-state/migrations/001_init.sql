-- Migration 001: RigOS Time-Travel Storage (immutable DAG)

CREATE TABLE IF NOT EXISTS traces (
    trace_id        UUID PRIMARY KEY,
    session_id      UUID NOT NULL,
    tenant_id       TEXT,
    created_at      TIMESTAMPTZ DEFAULT NOW(),
    root_prompt     TEXT NOT NULL,
    final_status    TEXT NOT NULL,              -- 'success' | 'failed' | 'human_approval'
    total_tokens    BIGINT NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS agent_steps (
    step_id           UUID PRIMARY KEY,
    trace_id          UUID NOT NULL REFERENCES traces(trace_id) ON DELETE CASCADE,
    parent_step_id    UUID REFERENCES agent_steps(step_id),
    step_number       INTEGER NOT NULL,
    payload           JSONB NOT NULL,
    tool_calls        JSONB,
    output            JSONB,
    requires_approval BOOLEAN NOT NULL DEFAULT false,
    created_at        TIMESTAMPTZ DEFAULT NOW(),

    CONSTRAINT unique_trace_step UNIQUE (trace_id, step_number)
);

-- Optimized indices for replay and branching (critical for performance)
CREATE INDEX IF NOT EXISTS idx_trace_parent ON agent_steps (trace_id, parent_step_id);
CREATE INDEX IF NOT EXISTS idx_trace_step_number ON agent_steps (trace_id, step_number);
