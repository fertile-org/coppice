CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE projects (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE repos (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    remote_url TEXT,
    default_branch TEXT NOT NULL DEFAULT 'main',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX repos_project_id_idx ON repos(project_id);

CREATE TABLE agent_presets (
    id UUID PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL,
    skills TEXT[] NOT NULL DEFAULT '{}',
    responsibilities TEXT[] NOT NULL DEFAULT '{}',
    system_prompt_template TEXT NOT NULL
);

CREATE TABLE agents (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    role TEXT NOT NULL,
    skills TEXT[] NOT NULL DEFAULT '{}',
    responsibilities TEXT[] NOT NULL DEFAULT '{}',
    system_prompt TEXT NOT NULL,
    provider_id TEXT NOT NULL DEFAULT 'mock',
    enabled BOOLEAN NOT NULL DEFAULT true,
    preset_source TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE tickets (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    repo_id UUID REFERENCES repos(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL,
    substatus TEXT,
    substatus_metadata JSONB,
    priority TEXT,
    assignee_agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    owner_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    branch_name TEXT,
    created_by TEXT NOT NULL,
    created_by_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX tickets_project_id_idx ON tickets(project_id);
CREATE INDEX tickets_status_idx ON tickets(project_id, status);

CREATE TABLE ticket_comments (
    id UUID PRIMARY KEY,
    ticket_id UUID NOT NULL REFERENCES tickets(id) ON DELETE CASCADE,
    author_type TEXT NOT NULL,
    author_id UUID,
    body TEXT NOT NULL,
    intent TEXT NOT NULL DEFAULT 'progress_update',
    mentions JSONB NOT NULL DEFAULT '[]',
    attachment_ids UUID[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX ticket_comments_ticket_id_idx ON ticket_comments(ticket_id);

CREATE TABLE attachments (
    id UUID PRIMARY KEY,
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    storage_path TEXT NOT NULL,
    uploaded_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

INSERT INTO agent_presets (id, key, role, skills, responsibilities, system_prompt_template) VALUES
  (gen_random_uuid(), 'pm', 'PM', ARRAY['planning','requirements'], ARRAY['refine tickets','prioritize backlog'], 'You are the PM Agent for Coppice.'),
  (gen_random_uuid(), 'tech_lead', 'Technical Lead', ARRAY['architecture','review'], ARRAY['guide implementation','review designs'], 'You are the Technical Lead Agent.'),
  (gen_random_uuid(), 'frontend_engineer', 'Frontend Engineer', ARRAY['react','css'], ARRAY['implement frontend tickets'], 'You are the Frontend Engineer Agent.'),
  (gen_random_uuid(), 'backend_engineer', 'Backend Engineer', ARRAY['rust','sql'], ARRAY['implement backend tickets'], 'You are the Backend Engineer Agent.'),
  (gen_random_uuid(), 'dba', 'DBA', ARRAY['postgres','migrations'], ARRAY['monitor database health'], 'You are the DBA Agent.'),
  (gen_random_uuid(), 'qc', 'QC', ARRAY['testing','qa'], ARRAY['verify quality'], 'You are the QC Agent.'),
  (gen_random_uuid(), 'reviewer', 'Reviewer', ARRAY['code review'], ARRAY['review changes'], 'You are the Reviewer Agent.'),
  (gen_random_uuid(), 'devops', 'DevOps', ARRAY['ci','deploy'], ARRAY['maintain pipelines'], 'You are the DevOps Agent.'),
  (gen_random_uuid(), 'security', 'Security', ARRAY['security review'], ARRAY['audit changes'], 'You are the Security Agent.'),
  (gen_random_uuid(), 'research', 'Research', ARRAY['investigation'], ARRAY['spike unknowns'], 'You are the Research Agent.');
