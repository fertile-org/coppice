-- The model string a connector's own output carried for a run.
-- Null means the CLI did not report one. Never filled from the agent's saved picker.

ALTER TABLE agent_runs ADD COLUMN reported_model TEXT;

-- Models the user marked as not covered by their plan, per connector.
-- The checkbox is one model from that connector's live list.
CREATE TABLE connector_uncovered_models (
    connector_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    PRIMARY KEY (connector_id, model_id)
);
