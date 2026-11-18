-- Firms (docs/design/approach-and-firms.md section 2). Simulated env only:
-- payouts are ledger transfers between agents' available balances.

CREATE TABLE firms (
    id              UUID PRIMARY KEY,
    env             TEXT NOT NULL CHECK (env = 'simulated'),
    name            TEXT NOT NULL,
    mandate         TEXT NOT NULL,
    lead_agent_id   UUID NOT NULL REFERENCES agents (id),
    lead_share_bps  INT NOT NULL CHECK (lead_share_bps BETWEEN 5000 AND 10000),
    task_fee        BIGINT NOT NULL CHECK (task_fee >= 0),
    -- [{role, slots, weight_bps}], validated by core::firm::FirmTerms.
    roles           JSONB NOT NULL,
    -- An approach `markets` object, or NULL for the lead's own.
    markets         JSONB,
    state           TEXT NOT NULL CHECK (state IN ('open', 'closed')),
    created_at      TIMESTAMPTZ NOT NULL,
    closed_at       TIMESTAMPTZ,
    CHECK ((state = 'closed') = (closed_at IS NOT NULL))
);

-- A lead runs at most one open firm.
CREATE UNIQUE INDEX firms_one_open_per_lead ON firms (lead_agent_id) WHERE state = 'open';

CREATE TABLE firm_members (
    firm_id    UUID NOT NULL REFERENCES firms (id),
    agent_id   UUID NOT NULL REFERENCES agents (id),
    role       TEXT NOT NULL CHECK (role IN ('scout', 'analyst')),
    joined_at  TIMESTAMPTZ NOT NULL,
    left_at    TIMESTAMPTZ,
    PRIMARY KEY (firm_id, agent_id, joined_at)
);

-- An agent is a current member of at most one firm.
CREATE UNIQUE INDEX firm_members_one_current ON firm_members (agent_id) WHERE left_at IS NULL;

ALTER TABLE interactions ADD COLUMN firm_id UUID REFERENCES firms (id);
CREATE INDEX interactions_firm_idx ON interactions (firm_id) WHERE firm_id IS NOT NULL;

CREATE TABLE firm_contributions (
    id              UUID PRIMARY KEY,
    seq             BIGSERIAL NOT NULL UNIQUE,
    firm_id         UUID NOT NULL REFERENCES firms (id),
    agent_id        UUID NOT NULL REFERENCES agents (id),
    role            TEXT NOT NULL CHECK (role IN ('scout', 'analyst')),
    kind            TEXT NOT NULL CHECK (kind IN ('scout_candidate', 'analyst_signal')),
    market_id       TEXT NOT NULL,
    market_view     JSONB,
    side            TEXT CHECK (side IN ('above', 'below')),
    conviction_bps  INT NOT NULL CHECK (conviction_bps BETWEEN 0 AND 10000),
    interaction_id  UUID REFERENCES interactions (id),
    used            BOOLEAN NOT NULL DEFAULT false,
    fee_paid        BIGINT NOT NULL DEFAULT 0 CHECK (fee_paid >= 0),
    payout          BIGINT NOT NULL DEFAULT 0 CHECK (payout >= 0),
    summary         TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL,
    CHECK (used = (interaction_id IS NOT NULL))
);

CREATE INDEX firm_contributions_firm_idx ON firm_contributions (firm_id, seq);
CREATE INDEX firm_contributions_interaction_idx ON firm_contributions (interaction_id)
    WHERE interaction_id IS NOT NULL;

ALTER TABLE activity ADD COLUMN firm_id UUID REFERENCES firms (id);
CREATE INDEX activity_firm_idx ON activity (firm_id) WHERE firm_id IS NOT NULL;
