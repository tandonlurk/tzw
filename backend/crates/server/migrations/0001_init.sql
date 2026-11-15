-- Core schema. Balances and stats are never stored: they are computed from
-- ledger_entries.

CREATE FUNCTION forbid_mutation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION '% is append-only', TG_TABLE_NAME;
END
$$;

CREATE TABLE agents (
    id              UUID PRIMARY KEY,
    name            TEXT NOT NULL,
    description     TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('active', 'paused')),
    env             TEXT NOT NULL CHECK (env IN ('arc_testnet', 'simulated')),
    wallet_address  TEXT,
    current_version INT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Immutable: editing an agent inserts a new row, history is never rewritten.
CREATE TABLE agent_versions (
    agent_id   UUID NOT NULL REFERENCES agents (id),
    version    INT NOT NULL CHECK (version >= 1),
    config     JSONB NOT NULL,
    policy     JSONB NOT NULL,
    graph      JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent_id, version)
);

CREATE TRIGGER agent_versions_immutable
    BEFORE UPDATE OR DELETE ON agent_versions
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

-- One row per posting. All rows of a posting set share the idempotency key
-- and sum to zero; (idempotency_key, line) makes a replay impossible.
CREATE TABLE ledger_entries (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    seq             BIGSERIAL NOT NULL UNIQUE,
    ts              TIMESTAMPTZ NOT NULL DEFAULT now(),
    env             TEXT NOT NULL CHECK (env IN ('arc_testnet', 'simulated')),
    idempotency_key TEXT NOT NULL,
    line            INT NOT NULL,
    account         TEXT NOT NULL,
    agent_id        UUID REFERENCES agents (id),
    delta           BIGINT NOT NULL CHECK (delta <> 0),
    reason          TEXT NOT NULL,
    interaction_id  UUID,
    tx_hash         TEXT,
    UNIQUE (idempotency_key, line)
);

CREATE INDEX ledger_entries_account_idx ON ledger_entries (env, account);
CREATE INDEX ledger_entries_agent_idx ON ledger_entries (agent_id);

CREATE TRIGGER ledger_entries_immutable
    BEFORE UPDATE OR DELETE ON ledger_entries
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

CREATE TABLE activity (
    id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    seq      BIGSERIAL NOT NULL UNIQUE,
    ts       TIMESTAMPTZ NOT NULL DEFAULT now(),
    agent_id UUID REFERENCES agents (id),
    kind     TEXT NOT NULL,
    summary  TEXT NOT NULL,
    detail   JSONB
);

CREATE INDEX activity_agent_idx ON activity (agent_id);

CREATE TABLE templates (
    id          UUID PRIMARY KEY,
    position    INT NOT NULL,
    name        TEXT NOT NULL,
    description TEXT NOT NULL,
    config      JSONB NOT NULL,
    policy      JSONB NOT NULL,
    graph       JSONB NOT NULL
);
