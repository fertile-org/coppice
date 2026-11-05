-- A later plan with no visible change retires live plan-run option sets
-- without publishing a replacement. NULL means the set is still live.
ALTER TABLE design_sets
    ADD COLUMN retired_at TIMESTAMPTZ;
