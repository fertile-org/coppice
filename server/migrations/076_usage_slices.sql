-- M15 D9: the shape Settings → Usage reads, whatever a connector reports.

-- When a ticket last moved to Done; cleared when it leaves Done. Tickets that
-- were Done before this column take their last update time, which is an
-- approximation: the move to Done was not recorded.
ALTER TABLE tickets ADD COLUMN done_at TIMESTAMPTZ;
UPDATE tickets SET done_at = updated_at WHERE status = 'done';

CREATE OR REPLACE FUNCTION set_ticket_done_at()
RETURNS trigger AS $$
BEGIN
    IF NEW.status = 'done' AND OLD.status IS DISTINCT FROM 'done' THEN
        NEW.done_at := now();
    ELSIF NEW.status IS DISTINCT FROM 'done' THEN
        NEW.done_at := NULL;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER tickets_set_done_at
BEFORE UPDATE OF status ON tickets
FOR EACH ROW EXECUTE FUNCTION set_ticket_done_at();

CREATE INDEX run_llm_requests_created_at_idx ON run_llm_requests (created_at);
CREATE INDEX run_subagents_created_at_idx ON run_subagents (created_at);
CREATE INDEX run_usage_updated_at_idx ON run_usage (updated_at);
CREATE INDEX run_usage_root_ticket_idx ON run_usage (root_ticket_id);

-- A slice is a part of one tracked run's total, with a model, a time and a
-- source. The slices of a run add up to its total:
--   request    each request (or turn) row, at its time
--   subagent   each subagent with a breakdown; one with only a total is
--              `unsplit_tokens`, unless the run's total was reported, which
--              already holds it
--   remainder  a reported total minus the rows above, at the run's last
--              snapshot. Split by model when the run's split by model covers
--              every row's model and each model's rows fit inside it; else one
--              slice with the run's reported model.
-- A remainder below zero (rows above the reported total) is clamped to 0.
CREATE VIEW usage_slices AS
WITH
subagent_rows AS (
    SELECT s.*,
           COALESCE(s.input_tokens, s.cache_read_tokens, s.cache_write_tokens,
                    s.output_tokens, s.reasoning_tokens) IS NOT NULL AS broken_down
    FROM run_subagents s
),
row_slices AS (
    SELECT r.usage_id, 'request'::text AS source, r.model, r.created_at AS at,
           r.input_tokens, r.cache_read_tokens, r.cache_write_tokens,
           r.output_tokens, r.reasoning_tokens, NULL::int8 AS unsplit_tokens
    FROM run_llm_requests r
    UNION ALL
    SELECT s.usage_id, 'subagent', s.model, s.created_at,
           s.input_tokens, s.cache_read_tokens, s.cache_write_tokens,
           s.output_tokens, s.reasoning_tokens,
           CASE WHEN s.broken_down THEN NULL ELSE s.total_tokens END
    FROM subagent_rows s
    JOIN run_usage u ON u.id = s.usage_id
    WHERE s.broken_down OR u.status <> 'reported'
),
rows_by_model AS (
    SELECT usage_id, model,
           sum(input_tokens) AS input_tokens,
           sum(cache_read_tokens) AS cache_read_tokens,
           sum(cache_write_tokens) AS cache_write_tokens,
           sum(output_tokens) AS output_tokens,
           sum(reasoning_tokens) AS reasoning_tokens
    FROM row_slices
    GROUP BY usage_id, model
),
rows_total AS (
    SELECT usage_id,
           sum(input_tokens) AS input_tokens,
           sum(cache_read_tokens) AS cache_read_tokens,
           sum(cache_write_tokens) AS cache_write_tokens,
           sum(output_tokens) AS output_tokens,
           sum(reasoning_tokens) AS reasoning_tokens
    FROM rows_by_model
    GROUP BY usage_id
),
split_by_model AS (
    SELECT u.id AS usage_id
    FROM run_usage u
    WHERE u.status = 'reported'
      AND EXISTS (SELECT 1 FROM run_usage_models m WHERE m.usage_id = u.id)
      AND NOT EXISTS (
          SELECT 1
          FROM rows_by_model r
          LEFT JOIN run_usage_models m ON m.usage_id = r.usage_id AND m.model = r.model
          WHERE r.usage_id = u.id
            AND (
                m.model IS NULL
                OR COALESCE(r.input_tokens, 0) > COALESCE(m.input_tokens, 0)
                OR COALESCE(r.cache_read_tokens, 0) > COALESCE(m.cache_read_tokens, 0)
                OR COALESCE(r.cache_write_tokens, 0) > COALESCE(m.cache_write_tokens, 0)
                OR COALESCE(r.output_tokens, 0) > COALESCE(m.output_tokens, 0)
                OR COALESCE(r.reasoning_tokens, 0) > COALESCE(m.reasoning_tokens, 0)
            )
      )
),
remainders AS (
    SELECT m.usage_id, m.model,
           m.input_tokens - COALESCE(r.input_tokens, 0) AS input_tokens,
           m.cache_read_tokens - COALESCE(r.cache_read_tokens, 0) AS cache_read_tokens,
           m.cache_write_tokens - COALESCE(r.cache_write_tokens, 0) AS cache_write_tokens,
           m.output_tokens - COALESCE(r.output_tokens, 0) AS output_tokens,
           m.reasoning_tokens - COALESCE(r.reasoning_tokens, 0) AS reasoning_tokens
    FROM run_usage_models m
    JOIN split_by_model USING (usage_id)
    LEFT JOIN rows_by_model r ON r.usage_id = m.usage_id AND r.model = m.model
    UNION ALL
    SELECT u.id, u.reported_model,
           GREATEST(u.input_tokens - COALESCE(t.input_tokens, 0), 0),
           GREATEST(u.cache_read_tokens - COALESCE(t.cache_read_tokens, 0), 0),
           GREATEST(u.cache_write_tokens - COALESCE(t.cache_write_tokens, 0), 0),
           GREATEST(u.output_tokens - COALESCE(t.output_tokens, 0), 0),
           GREATEST(u.reasoning_tokens - COALESCE(t.reasoning_tokens, 0), 0)
    FROM run_usage u
    LEFT JOIN rows_total t ON t.usage_id = u.id
    WHERE u.status = 'reported'
      AND NOT EXISTS (SELECT 1 FROM split_by_model sm WHERE sm.usage_id = u.id)
),
slices AS (
    SELECT * FROM row_slices
    UNION ALL
    SELECT r.usage_id, 'remainder', r.model, u.updated_at,
           r.input_tokens, r.cache_read_tokens, r.cache_write_tokens,
           r.output_tokens, r.reasoning_tokens, NULL::int8
    FROM remainders r
    JOIN run_usage u ON u.id = r.usage_id
    WHERE COALESCE(r.input_tokens, 0) + COALESCE(r.cache_read_tokens, 0)
        + COALESCE(r.cache_write_tokens, 0) + COALESCE(r.output_tokens, 0) > 0
)
SELECT s.usage_id, s.source, s.model, s.at,
       s.input_tokens, s.cache_read_tokens, s.cache_write_tokens,
       s.output_tokens, s.reasoning_tokens, s.unsplit_tokens,
       COALESCE(s.input_tokens, 0) + COALESCE(s.cache_read_tokens, 0)
           + COALESCE(s.cache_write_tokens, 0) + COALESCE(s.output_tokens, 0)
           + COALESCE(s.unsplit_tokens, 0) AS total_tokens,
       u.connector, u.agent_id, u.agent_name, u.job_type, u.ticket_id, u.root_ticket_id,
       u.board_id, u.board_name, u.chat_session_id
FROM slices s
JOIN run_usage u ON u.id = s.usage_id;
