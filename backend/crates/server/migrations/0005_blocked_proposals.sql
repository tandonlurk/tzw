-- A proposal attempt that was denied by `authorize` or found no counterparty
-- creates no interaction, so the proposal cooldown (keyed on interactions)
-- did not see it. The discovery run is marked instead, with the market it
-- was about, and the cooldown counts it too (only under the same agent
-- version: an owner edit lets the agent try again at once).
ALTER TABLE agent_runs ADD COLUMN market_key TEXT;
ALTER TABLE agent_runs ADD COLUMN blocked TEXT
    CHECK (blocked IN ('denied', 'no_counterparty'));
ALTER TABLE agent_runs ADD CONSTRAINT agent_runs_blocked_has_market
    CHECK (blocked IS NULL OR market_key IS NOT NULL);
CREATE INDEX agent_runs_blocked_idx ON agent_runs (agent_id, market_key, ts)
    WHERE blocked IS NOT NULL;

-- When the challenge settled, on the lifecycle clock (the simulated `now` of
-- a dev tick, real time otherwise). Ledger rows keep their own wall-clock
-- `ts`; P&L over time is plotted against this.
ALTER TABLE interactions ADD COLUMN settled_at TIMESTAMPTZ;
UPDATE interactions SET settled_at = observed_at WHERE state = 'settled';
