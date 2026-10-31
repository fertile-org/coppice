ALTER TABLE ticket_plans
    ADD COLUMN design_set_id UUID REFERENCES design_sets(id) ON DELETE SET NULL,
    ADD COLUMN design_option_id TEXT;
