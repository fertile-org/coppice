-- Needs-action notifications for a Done ticket whose git flow stopped.
ALTER TABLE notifications DROP CONSTRAINT notifications_type_check;
ALTER TABLE notifications
    ADD CONSTRAINT notifications_type_check CHECK (
        type IN (
            'agent_run_finished',
            'agent_mentioned',
            'knowledge_compaction_failed',
            'git_needs_action'
        )
    );
