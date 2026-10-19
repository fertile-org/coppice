-- The live console format a run used. NULL means the connector's default.
-- OpenCode 2 runs (over ACP) use the structured console while 1.x runs keep
-- the OpenCode session view, so the connector alone can't tell.
ALTER TABLE agent_runs ADD COLUMN console_kind TEXT;
