-- M2: challenge lifecycle. `interactions.state` is the source of truth and
-- only moves along the table in aen_core::interaction.

CREATE TABLE interactions (
    id                     UUID PRIMARY KEY,
    kind                   TEXT NOT NULL CHECK (kind IN ('challenge', 'collaboration')),
    env                    TEXT NOT NULL CHECK (env IN ('arc_testnet', 'simulated')),
    state                  TEXT NOT NULL,
    proposer_id            UUID NOT NULL REFERENCES agents (id),
    counterparty_id        UUID NOT NULL REFERENCES agents (id),
    proposer_version       INT NOT NULL,
    counterparty_version   INT NOT NULL,
    -- Current terms (aen_core::challenge::ChallengeTerms). stake, market_key,
    -- start_ts and end_ts are copies of fields in `terms`, kept for queries.
    terms                  JSONB NOT NULL,
    stake                  BIGINT NOT NULL CHECK (stake > 0),
    market_key             TEXT NOT NULL,
    market_title           TEXT NOT NULL,
    start_ts               TIMESTAMPTZ NOT NULL,
    end_ts                 TIMESTAMPTZ NOT NULL,
    -- Whose move it is while Proposed / Negotiating.
    turn                   UUID REFERENCES agents (id),
    -- Offers made so far (the proposal is round 1).
    rounds                 INT NOT NULL DEFAULT 1,
    -- Owner preference from the graph (action.request_approval): ask even
    -- when the policy would allow the stake automatically.
    proposer_ask_owner     BOOLEAN NOT NULL DEFAULT FALSE,
    counterparty_ask_owner BOOLEAN NOT NULL DEFAULT FALSE,
    outcome                TEXT CHECK (outcome IN ('a', 'b', 'void')),
    winner_id              UUID REFERENCES agents (id),
    observed               BIGINT,
    observed_at            TIMESTAMPTZ,
    resolve_attempts       INT NOT NULL DEFAULT 0,
    status_reason          TEXT,
    created_at             TIMESTAMPTZ NOT NULL,
    updated_at             TIMESTAMPTZ NOT NULL,
    CHECK (proposer_id <> counterparty_id)
);

CREATE INDEX interactions_state_idx ON interactions (state);
CREATE INDEX interactions_proposer_idx ON interactions (proposer_id);
CREATE INDEX interactions_counterparty_idx ON interactions (counterparty_id);

-- Every offer and answer, append-only.
CREATE TABLE negotiation_rounds (
    interaction_id UUID NOT NULL REFERENCES interactions (id),
    seq            INT NOT NULL,
    agent_id       UUID NOT NULL REFERENCES agents (id),
    action         TEXT NOT NULL CHECK (action IN ('propose', 'counter', 'accept', 'reject')),
    terms          JSONB NOT NULL,
    summary        TEXT NOT NULL,
    ts             TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (interaction_id, seq)
);

CREATE TRIGGER negotiation_rounds_immutable
    BEFORE UPDATE OR DELETE ON negotiation_rounds
    FOR EACH ROW EXECUTE FUNCTION forbid_mutation();

-- Owner approval of one agent's stake in one interaction. An approval never
-- moves funds by itself: authorize() runs again when it is granted.
CREATE TABLE approvals (
    id             UUID PRIMARY KEY,
    interaction_id UUID NOT NULL REFERENCES interactions (id),
    agent_id       UUID NOT NULL REFERENCES agents (id),
    stake          BIGINT NOT NULL,
    status         TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'rejected', 'expired', 'superseded')),
    reason         TEXT NOT NULL,
    requested_at   TIMESTAMPTZ NOT NULL,
    decided_at     TIMESTAMPTZ,
    UNIQUE (interaction_id, agent_id)
);

-- One row per graph run: a concise trace (decision summaries, never model
-- chain-of-thought) and the intents it produced.
CREATE TABLE agent_runs (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    seq            BIGSERIAL NOT NULL UNIQUE,
    ts             TIMESTAMPTZ NOT NULL,
    agent_id       UUID NOT NULL REFERENCES agents (id),
    agent_version  INT NOT NULL,
    trigger        TEXT NOT NULL,
    interaction_id UUID REFERENCES interactions (id),
    status         TEXT NOT NULL CHECK (status IN ('ok', 'no_action', 'error')),
    error          TEXT,
    reasoner       TEXT NOT NULL,
    trace          JSONB NOT NULL,
    intents        JSONB NOT NULL
);

CREATE INDEX agent_runs_agent_idx ON agent_runs (agent_id, trigger, seq DESC);
CREATE INDEX agent_runs_interaction_idx ON agent_runs (interaction_id);

ALTER TABLE activity ADD COLUMN interaction_id UUID REFERENCES interactions (id);
CREATE INDEX activity_interaction_idx ON activity (interaction_id);

-- SEC-03: a stake enters an agent's committed account once per interaction
-- and leaves it once (by settlement or by refund), whatever idempotency key a
-- writer uses. The lifecycle also holds a row lock on the interaction and
-- checks its state; these indexes are the last line of defence.
CREATE UNIQUE INDEX ledger_one_commit_per_interaction
    ON ledger_entries (interaction_id, account)
    WHERE reason = 'commit_stake' AND delta > 0;
CREATE UNIQUE INDEX ledger_one_release_per_interaction
    ON ledger_entries (interaction_id, account)
    WHERE reason IN ('settle', 'refund') AND delta < 0 AND account LIKE 'agent:%:committed';
