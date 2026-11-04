-- Per-agent thinking level on a shared scale. Each connector maps it to its own
-- flag; NULL leaves the choice to the CLI.
ALTER TABLE agents
  ADD COLUMN IF NOT EXISTS thinking_level TEXT NULL
  CHECK (thinking_level IN ('low', 'medium', 'high', 'max'));
