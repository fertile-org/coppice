-- M15 D6: a run's reported total split by model, when the CLI reports it that
-- way (Claude Code `modelUsage`). Rewritten with every snapshot of the usage
-- row; the rows add up to the row's totals.
CREATE TABLE run_usage_models (
    usage_id UUID NOT NULL REFERENCES run_usage(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    input_tokens BIGINT,
    cache_read_tokens BIGINT,
    cache_write_tokens BIGINT,
    output_tokens BIGINT,
    reasoning_tokens BIGINT,
    PRIMARY KEY (usage_id, model)
);
