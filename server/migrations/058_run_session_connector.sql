-- The connector that created a run's session. Session ids only mean something
-- to that connector, and an agent can switch connector between runs. NULL rows
-- predate this column: they start a new session instead of resuming.
ALTER TABLE agent_runs ADD COLUMN session_connector TEXT;
