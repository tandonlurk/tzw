-- Arc testnet wiring: an agent may be linked to an onchain AgentVault (D-57),
-- and challenges between two vault agents are staked through ChallengeEscrow.
-- Addresses and hashes are stored lowercase.

ALTER TABLE agents
    ADD COLUMN vault_address TEXT CHECK (vault_address ~ '^0x[0-9a-f]{40}$'),
    ADD COLUMN vault_owner   TEXT CHECK (vault_owner ~ '^0x[0-9a-f]{40}$'),
    ADD CHECK ((vault_address IS NULL) = (vault_owner IS NULL));

-- One vault belongs to one agent.
CREATE UNIQUE INDEX agents_vault_address_key ON agents (vault_address)
    WHERE vault_address IS NOT NULL;

-- Onchain transactions of an arc_testnet challenge, written after each one is
-- confirmed. The escrow itself stays the source of truth: every tick reads it
-- before acting, so a hash missing here (crash after send) only loses a link.
ALTER TABLE interactions
    ADD COLUMN escrow_create_tx      TEXT CHECK (escrow_create_tx ~ '^0x[0-9a-f]{64}$'),
    ADD COLUMN proposer_stake_tx     TEXT CHECK (proposer_stake_tx ~ '^0x[0-9a-f]{64}$'),
    ADD COLUMN counterparty_stake_tx TEXT CHECK (counterparty_stake_tx ~ '^0x[0-9a-f]{64}$'),
    -- resolve(winner) or resolveVoid
    ADD COLUMN escrow_settle_tx      TEXT CHECK (escrow_settle_tx ~ '^0x[0-9a-f]{64}$'),
    -- The outcome decided from market data, kept while the escrow call is
    -- retried, so a retry never re-reads the market and decides differently.
    ADD COLUMN pending_outcome       JSONB,
    -- Both vaults' exposure released after the escrow reached a final state.
    ADD COLUMN vaults_released_at    TIMESTAMPTZ;

-- A verified chain log is posted at most once (keys `chain:{tx}:{log_index}`
-- are unique per env already); this makes the tx lookup cheap.
CREATE INDEX ledger_entries_tx_hash_idx ON ledger_entries (tx_hash) WHERE tx_hash IS NOT NULL;
