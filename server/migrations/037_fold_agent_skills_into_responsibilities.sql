-- Fold free-text skill labels into Responsibilities, then stop storing them.
-- Each label is appended, in order, unless that exact line is already present.
-- Repeated labels in the skills list are appended once.

UPDATE agents
SET responsibilities = responsibilities || COALESCE((
    SELECT array_agg(skill ORDER BY ord)
    FROM (
        SELECT skill, min(ord) AS ord
        FROM unnest(skills) WITH ORDINALITY AS incoming(skill, ord)
        WHERE NOT (skill = ANY (responsibilities))
        GROUP BY skill
    ) unique_skills
), ARRAY[]::text[]);

UPDATE agent_presets
SET responsibilities = responsibilities || COALESCE((
    SELECT array_agg(skill ORDER BY ord)
    FROM (
        SELECT skill, min(ord) AS ord
        FROM unnest(skills) WITH ORDINALITY AS incoming(skill, ord)
        WHERE NOT (skill = ANY (responsibilities))
        GROUP BY skill
    ) unique_skills
), ARRAY[]::text[]);

ALTER TABLE agents DROP COLUMN skills;
ALTER TABLE agent_presets DROP COLUMN skills;
