ALTER TABLE agents RENAME COLUMN provider TO connector;

ALTER TABLE agents
  ADD COLUMN IF NOT EXISTS model_provider TEXT NULL;

-- Split legacy composite model values (provider/model) into separate columns
UPDATE agents
SET
  model_provider = split_part(model, '/', 1),
  model = NULLIF(split_part(model, '/', 2), '')
WHERE model IS NOT NULL AND position('/' IN model) > 0;
