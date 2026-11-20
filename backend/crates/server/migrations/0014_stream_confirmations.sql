-- QuickNode Streams as an independent confirmation of the Arc testnet
-- lifecycle (D-74). Escrow and vault-factory logs are decoded and matched to
-- the challenge (escrow id = interaction id) or agent (agentRef = agent id)
-- they belong to, then compared with the transactions the server confirmed
-- from its own receipts. A stream event never moves state or money.

ALTER TABLE chain_events
    -- `blockTimestamp` of the log when the delivery carries it (Arc's logs do).
    ADD COLUMN block_timestamp TIMESTAMPTZ,
    -- Decoded `ChallengeEscrow` / `VaultFactory` event; NULL for other logs.
    ADD COLUMN event_name      TEXT,
    ADD COLUMN decoded         JSONB,
    ADD COLUMN interaction_id  UUID REFERENCES interactions (id),
    ADD COLUMN agent_id        UUID REFERENCES agents (id),
    -- pending:   the server has not confirmed this step yet (rechecked each tick)
    -- confirmed: same transaction and values as the server's verified receipt
    -- mismatch:  disagrees with what the server confirmed (flagged, nothing moves)
    -- observed:  a real event with no server-side counterpart (e.g. a timeout refund)
    -- unmatched: no challenge or agent on this server has this id
    ADD COLUMN match_status    TEXT CHECK (match_status IN
        ('pending', 'confirmed', 'mismatch', 'observed', 'unmatched')),
    ADD COLUMN match_note      TEXT,
    ADD COLUMN checked_at      TIMESTAMPTZ,
    ADD CHECK ((event_name IS NULL) = (match_status IS NULL));

CREATE INDEX chain_events_interaction_idx ON chain_events (interaction_id)
    WHERE interaction_id IS NOT NULL;
CREATE INDEX chain_events_pending_idx ON chain_events (seq)
    WHERE match_status = 'pending';
