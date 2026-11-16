-- Two-agent collaboration agreements (simulated funds). Every fund movement
-- is a ledger posting set authorized by `core::authorize`; this table holds
-- the agreement and its state, the ledger holds the money.
CREATE TABLE collaborations (
    id                    UUID PRIMARY KEY,
    seq                   BIGSERIAL NOT NULL UNIQUE,
    env                   TEXT NOT NULL CHECK (env IN ('arc_testnet', 'simulated')),
    state                 TEXT NOT NULL CHECK (state IN ('proposed', 'accepted', 'rejected',
                                                         'funded', 'completed', 'cancelled')),
    proposer_id           UUID NOT NULL REFERENCES agents (id),
    partner_id            UUID NOT NULL REFERENCES agents (id),
    proposer_version      INT NOT NULL,
    partner_version       INT,
    objective             TEXT NOT NULL,
    proposer_share_bps    INT NOT NULL CHECK (proposer_share_bps BETWEEN 0 AND 10000),
    proposer_contribution BIGINT NOT NULL CHECK (proposer_contribution > 0),
    partner_contribution  BIGINT NOT NULL CHECK (partner_contribution > 0),
    -- Set on completion; always proposer_payout + partner_payout = pool.
    proposer_payout       BIGINT,
    partner_payout        BIGINT,
    status_reason         TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    funded_at             TIMESTAMPTZ,
    closed_at             TIMESTAMPTZ,
    CHECK (proposer_id <> partner_id),
    CHECK ((proposer_payout IS NULL) = (partner_payout IS NULL)),
    CHECK (proposer_payout IS NULL
           OR proposer_payout + partner_payout = proposer_contribution + partner_contribution)
);

CREATE INDEX collaborations_proposer_idx ON collaborations (proposer_id);
CREATE INDEX collaborations_partner_idx ON collaborations (partner_id);

ALTER TABLE activity ADD COLUMN collaboration_id UUID REFERENCES collaborations (id);
CREATE INDEX activity_collaboration_idx ON activity (collaboration_id);

ALTER TABLE ledger_entries ADD COLUMN collaboration_id UUID REFERENCES collaborations (id);
CREATE INDEX ledger_entries_collaboration_idx ON ledger_entries (collaboration_id);

-- Like SEC-03 for challenges: a contribution enters an agent's committed
-- account once per collaboration and leaves it once (payout or refund).
CREATE UNIQUE INDEX ledger_one_collab_commit
    ON ledger_entries (collaboration_id, account)
    WHERE reason = 'collab_commit' AND delta > 0;
CREATE UNIQUE INDEX ledger_one_collab_release
    ON ledger_entries (collaboration_id, account)
    WHERE reason IN ('collab_payout', 'collab_refund') AND delta < 0
      AND account LIKE 'agent:%:committed';
