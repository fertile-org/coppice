-- The ticket branch head an agent review passed. A pass covers only that code.
ALTER TABLE tickets
    ADD COLUMN agent_review_passed_sha TEXT;
