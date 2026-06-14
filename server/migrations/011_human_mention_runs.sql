ALTER TABLE agent_runs
  ADD COLUMN context_profile TEXT NOT NULL DEFAULT 'full',
  ADD COLUMN trigger_comment_id UUID REFERENCES ticket_comments(id) ON DELETE SET NULL;

ALTER TABLE agent_runs
  ADD CONSTRAINT agent_runs_context_profile_check
  CHECK (context_profile IN ('full', 'human_agent', 'human_chat'));
