-- Each vault's `release(challengeId)` after the escrow is final (TODO M3b:
-- these were sent but not recorded). NULL until confirmed, or forever when
-- the vault had nothing staked for the challenge (release is skipped).
ALTER TABLE interactions
    ADD COLUMN proposer_release_tx     TEXT CHECK (proposer_release_tx ~ '^0x[0-9a-f]{64}$'),
    ADD COLUMN counterparty_release_tx TEXT CHECK (counterparty_release_tx ~ '^0x[0-9a-f]{64}$');
