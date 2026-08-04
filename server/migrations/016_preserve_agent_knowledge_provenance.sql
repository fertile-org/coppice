-- Agent-scoped knowledge revisions are immutable historical records. Refuse
-- deletion of their source agent instead of cascading away provenance.
ALTER TABLE knowledge_revisions
    DROP CONSTRAINT knowledge_revisions_agent_id_fkey,
    ADD CONSTRAINT knowledge_revisions_agent_id_fkey
        FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE RESTRICT;
