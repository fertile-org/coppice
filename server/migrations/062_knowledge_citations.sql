-- Code citations on knowledge revisions, anchored to a default-branch commit.
CREATE TABLE knowledge_citations (
    id UUID PRIMARY KEY,
    revision_id UUID NOT NULL REFERENCES knowledge_revisions(id) ON DELETE CASCADE,
    repo_id UUID REFERENCES repos(id) ON DELETE SET NULL,
    path TEXT NOT NULL,
    commit_sha TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (revision_id, repo_id, path)
);
CREATE INDEX knowledge_citations_revision_idx ON knowledge_citations(revision_id);
