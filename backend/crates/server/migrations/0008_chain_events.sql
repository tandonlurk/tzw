-- On-chain events received from QuickNode Streams (local ingestion only; no
-- chain executor reads or acts on them yet). One row per log, keyed on
-- (tx_hash, log_index), so duplicate and reordered deliveries are no-ops.
CREATE TABLE chain_events (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    seq          BIGSERIAL NOT NULL UNIQUE,
    source       TEXT NOT NULL CHECK (source IN ('quicknode_streams')),
    network      TEXT,
    stream_id    TEXT,
    tx_hash      TEXT NOT NULL CHECK (tx_hash ~ '^0x[0-9a-f]{64}$'),
    log_index    BIGINT NOT NULL CHECK (log_index >= 0),
    block_number BIGINT NOT NULL CHECK (block_number >= 0),
    block_hash   TEXT,
    address      TEXT NOT NULL CHECK (address ~ '^0x[0-9a-f]{40}$'),
    topics       JSONB NOT NULL,
    data         TEXT NOT NULL,
    -- Set when a later delivery reports the log as removed by a reorg.
    removed      BOOLEAN NOT NULL DEFAULT false,
    received_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tx_hash, log_index)
);

CREATE INDEX chain_events_block_idx ON chain_events (block_number, log_index);
CREATE INDEX chain_events_address_idx ON chain_events (address);
