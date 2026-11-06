-- Token usage a connector reported for a run. Nothing here names a connector:
-- rows come from normalized usage events. A NULL count means "not reported".

-- Totals per run: the CLI's own total when it gave one ('reported'), else the
-- sum of the rows below ('partial'). No row means the run reported no usage.
CREATE TABLE run_usage (
    run_id UUID PRIMARY KEY REFERENCES agent_runs(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('reported', 'partial')),
    -- Grows with every change; a stale write never replaces a newer one.
    version BIGINT NOT NULL,
    input_tokens BIGINT,
    cache_read_tokens BIGINT,
    cache_write_tokens BIGINT,
    output_tokens BIGINT,
    reasoning_tokens BIGINT,
    request_count INTEGER NOT NULL DEFAULT 0,
    subagent_count INTEGER NOT NULL DEFAULT 0,
    -- Stored when the CLI reports it; not shown.
    reported_cost_usd NUMERIC(14, 6),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One LLM request (or turn, for CLIs that only report turns) on the run's own thread.
CREATE TABLE run_llm_requests (
    run_id UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    external_id TEXT,
    granularity TEXT NOT NULL CHECK (granularity IN ('request', 'turn')),
    model TEXT,
    input_tokens BIGINT,
    cache_read_tokens BIGINT,
    cache_write_tokens BIGINT,
    output_tokens BIGINT,
    reasoning_tokens BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, seq)
);

-- Subagents the connector spawned inside a run, with their own totals.
CREATE TABLE run_subagents (
    run_id UUID NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    seq INTEGER NOT NULL,
    external_id TEXT NOT NULL,
    agent_type TEXT,
    description TEXT,
    model TEXT,
    status TEXT,
    input_tokens BIGINT,
    cache_read_tokens BIGINT,
    cache_write_tokens BIGINT,
    output_tokens BIGINT,
    reasoning_tokens BIGINT,
    -- The CLI's own total, when it gave one without a breakdown.
    total_tokens BIGINT,
    tool_uses INTEGER,
    duration_ms BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, seq)
);
