-- Practice-only argument history. No reference to escrow, ledger or approvals.
CREATE TABLE sparring_sessions (
    id UUID PRIMARY KEY,
    agent_id UUID NOT NULL REFERENCES agents(id),
    agent_version INT NOT NULL,
    thesis TEXT NOT NULL CHECK (length(thesis) BETWEEN 1 AND 4000),
    agent_snapshot JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    FOREIGN KEY (agent_id, agent_version) REFERENCES agent_versions(agent_id, version)
);
CREATE INDEX sparring_sessions_agent ON sparring_sessions(agent_id, created_at DESC);
CREATE TABLE sparring_rounds (
    id UUID PRIMARY KEY,
    session_id UUID NOT NULL REFERENCES sparring_sessions(id),
    position INT NOT NULL CHECK (position BETWEEN 1 AND 12),
    role TEXT NOT NULL CHECK (role IN ('owner', 'adversary')),
    source TEXT NOT NULL CHECK (source IN ('owner', 'grok_bot')),
    argument TEXT NOT NULL CHECK (length(argument) BETWEEN 1 AND 6000),
    evidence_urls JSONB NOT NULL,
    proposed_revision TEXT CHECK (length(proposed_revision) BETWEEN 1 AND 4000),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(session_id, position),
    CHECK ((role = 'owner' AND source = 'owner') OR (role = 'adversary' AND source = 'grok_bot'))
);
CREATE TRIGGER sparring_sessions_immutable BEFORE UPDATE OR DELETE ON sparring_sessions
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER sparring_rounds_immutable BEFORE UPDATE OR DELETE ON sparring_rounds
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();
CREATE TRIGGER sparring_sessions_no_truncate BEFORE TRUNCATE ON sparring_sessions
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_truncate();
CREATE TRIGGER sparring_rounds_no_truncate BEFORE TRUNCATE ON sparring_rounds
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_truncate();
