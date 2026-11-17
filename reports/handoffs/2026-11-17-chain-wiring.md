# Handoff 2026-11-17 — Arc testnet wired into the challenge lifecycle (backend)

Branch `chain-wiring` (from `chain-vaults` 2f03581, fast-forwarded to `chain-vaults` 2f16d93 for the smoke-run retry layer). Not pushed. Covers the chain-vaults handoff "Next steps" 2–5.

## What was done

- **Ledger** (`core::ledger`): `Reason::VaultDeposit` (external → agent available) and `VaultWithdraw` (agent available → external), env `arc_testnet`, idempotency key `chain:{tx}:{log_index}`; `PostingSet::with_tx_hash` so entries carry their onchain tx (`ledger_entries.tx_hash`, already in the schema, now written).
- **Migration 0009**: `agents.vault_address/vault_owner` (lowercase hex, unique vault), interaction columns `escrow_create_tx`, `proposer_stake_tx`, `counterparty_stake_tx`, `escrow_settle_tx`, `pending_outcome` (JSONB), `vaults_released_at`.
- **Settings**: chain mode turns on only when `AEN_TESTNET_MNEMONIC` is set; then `ARC_RPC_URL`, `ARC_ESCROW_ADDRESS`, `ARC_VAULT_FACTORY_ADDRESS` are required (optional `ARC_USDC_ADDRESS`, `ARC_EXPLORER_URL`, `ARC_PUBLIC_RPC_URL`, `AEN_DEV_CHAIN_OWNER`). `AppState.chain: Option<Arc<ChainState>>`; `None` = everything simulated exactly as before (all pre-existing tests unchanged and passing).
- **Lifecycle** (`lifecycle.rs` + new `lifecycle_arc.rs`):
  - `try_commit` for `arc_testnet`: same `authorize` + ledger reservation, stops at `funded` (also requires both agents to still have vaults).
  - Fund phase: read escrow → `create` if missing (`fundDeadline` = `start_ts`, `resolveDeadline` = `end_ts` + `RESOLUTION_GRACE_SECS`) → per vault: preflight (paused, operator, caps, balance) → `vault_stake` → `active` once the escrow is Active. Any refusal or a passed deadline → `resolveVoid` (if anything deposited) → ledger refund → `cancelled`/`void`.
  - Resolve: outcome decided once from market data and stored in `pending_outcome`, then `resolve(winner vault)` / `resolveVoid`, then ledger settle via the same `apply_outcome` as simulated (settle split into `decide_outcome` + `apply_outcome`; simulated behaviour identical). The escrow's final state wins if it differs (timeout refund by anyone).
  - Release: frees both vaults' exposure once the escrow is final (in the same tick).
  - Arc interactions use the wall clock in every phase; the dev tick's simulated `now` only affects simulated ones. Funding deadlines are checked against max(wall, chain block time).
  - RPC errors leave state untouched and are reported as `chain_unavailable: …` in the tick report; the next tick reads the escrow first. Only a revert counts as the chain refusing.
- **Endpoints** (`server::chain`), per the contract: `GET /api/chain`, `GET|POST /api/agents/{id}/vault`, `POST /api/agents/{id}/vault/sync`, dev `POST /api/dev/chain/setup-demo-vaults`. Documented in `docs/api.md` "Arc testnet".
- **Labels**: amounts in activity and reasoner summaries say `(Arc testnet USDC)` for arc agents (`stake_text` takes the env; `AgentCtx.env` added).

## Files

`backend/crates/core/src/ledger.rs`; `backend/crates/server/{Cargo.toml, migrations/0009_chain_wiring.sql, src/chain.rs, src/lifecycle_arc.rs, src/lifecycle.rs, src/ledger_db.rs, src/state.rs, src/lib.rs, src/main.rs, src/agents.rs, src/models.rs, src/interactions.rs, src/reasoner.rs, src/graph_exec.rs, src/custom_code.rs, tests/chain.rs}`; `docs/api.md`; `backend/.env.example`; `backend/README.md`; `tasks/backend.md`. No change to `backend/crates/chain/src` or `web/`.

## How to run / test

- `git submodule update --init && (cd contracts && forge build)`, then `cd backend && cargo test --workspace` → **341 passed, 0 failed, 1 ignored** (new: 6 in `tests/chain.rs`, 1 core ledger test). `tests/chain.rs` spawns its own anvil (genesis 10 min in the past so block times stay behind wall-clock deadlines) behind a toggleable TCP proxy to simulate RPC outages. Never touches Arc.
- Manual E2E done with the real binary on a local anvil (contracts via `forge create`, anvil dev mnemonic): seed-demo → setup-demo-vaults → demo ticks → approve → active with 3 escrow txs → settled with resolve tx; bull vault 7.50 / skeptic 2.50, drift 0, exposure 0; mnemonic never in the log. Commands in `backend/README.md` "Arc testnet".

## Endpoint contract: deviations (all additive / backward compatible)

- `POST /vault/sync` response also has `skipped: [{log_index, amount, reason}]`. `credited` holds only the agent's own lines (deposits positive, withdrawals negative), not the `external` counter-lines.
- `GET /api/chain` `rpc_url` is `ARC_PUBLIC_RPC_URL` (default the public Arc RPC), not the backend's `ARC_RPC_URL`, which may contain a provider key. `chain_name` is "Local dev chain" on 31337. `explorer_url` can be null if unset (never in practice: defaults to the Arc explorer).
- `Agent` gains `vault_address`, `vault_owner`; `Interaction` gains `escrow: {create_tx, proposer_stake_tx, counterparty_stake_tx, settle_tx}|null`.
- Addresses in responses are checksummed; the DB and `Agent.vault_address` are lowercase. Compare case-insensitively.
- Web assumptions (a)–(e) from the lead all hold: drift sign as specified; withdrawals come back as negative `credited` entries; vault `explorer_url` = `{explorer}/address/{vault}`; `GET /api/chain` implemented; after a successful `POST /vault` the agent's `env` is `arc_testnet`.
- `POST /approvals/{id}/approve` returns the interaction `funded` (not `active`) for arc challenges; it turns `active` on the next tick.

## Proposed decisions (for docs/decisions.md)

| Default | Rationale |
|---|---|
| Chain mode is enabled by `AEN_TESTNET_MNEMONIC` alone; with it the other `ARC_*` are required and a connect failure stops startup | A copied `.env.example` (addresses only) cannot half-enable chain mode; a misconfigured chain fails loudly instead of leaving arc challenges stuck |
| Arc commit reserves in the ledger first (`commit_stake`), then stakes onchain; failure or deadline → `resolveVoid` + ledger `refund` + `cancelled`/`void` | `authorize` runs in the same DB transaction as the reservation (unchanged invariant); the chain step can be retried and never needs to undo a ledger write except through the normal refund |
| Escrow `resolveDeadline` = `end_ts` + `RESOLUTION_GRACE_SECS` (600 s) | The public timeout refund opens only after the backend's own retry window (+ the contract's 1 h GRACE) |
| Outcome decided once and stored (`pending_outcome`) before the onchain call; the escrow's final state overrides it in the ledger | A retry after a crash never re-reads the market and pays a different winner; the ledger always follows where the money actually went |
| Funding deadline checked against max(wall clock, chain block time) with a 2 s margin; resolution gated on the wall clock only | Each deadline is checked against the clock that enforces it; never resolve before `end_ts` in real time |
| Vault preflight (paused, operator, caps, balance) before staking; a revert or preflight failure voids at once rather than retrying until the deadline | Clear reason for the owner and stakes freed immediately; RPC errors (not reverts) are retried |
| `/vault/sync` ignores vault↔escrow transfers and skips withdrawals the ledger cannot cover (reported, visible as drift) | Stakes/payouts are the lifecycle's; the ledger never goes negative or invents entries |
| `GET /api/chain` exposes `ARC_PUBLIC_RPC_URL`, never `ARC_RPC_URL` | The backend RPC URL may embed a provider key (AGENTS.md 8.1) |
| Linking switches the agent to `arc_testnet` and leaves its simulated balance in the simulated ledger; refused with open challenges or collaborations | One env per agent keeps balances and exposure unambiguous |
| `setup-demo-vaults` default 5 USDC per agent (caps = amount) and renames "(simulated)" demo agents to "(Arc testnet)" | The demo flow stakes up to 5.00 (bull proposal) / 2.50 (skeptic counter); the old label would be wrong |
| One process-wide lock serializes all lifecycle chain sends | Resolver/operator send one tx at a time (D-59); avoids nonce races between the scheduler and `/dev/tick` |

## Assumptions

- Single server process (the chain lock is in-process; two servers with the same keys would race nonces).
- Arc block timestamps track the wall clock (smoke run). On anvil they can run ahead; tests start anvil in the past.
- Vault exposure briefly shows as drift between resolve and release; both happen in the same tick.

## Unfinished / next

- Live Arc run (lead): `AEN_DEV_CHAIN_OWNER=1`, `setup-demo-vaults` (test owner idx 2 needs testnet USDC), short horizons (README). Expect Arc RPC rate limits to surface as `chain_unavailable` tick errors that clear on the next tick.
- Withdrawals by the owner in the browser must be posted to `/vault/sync` by the UI; there is no chain watcher yet (QuickNode Streams later could call the same sync path).
- Collaborations remain simulated-only (refused for arc agents, unchanged).
- `GET /wallet` and `/stats/portfolio` still report the simulated env only.

## Human review

- Everything above is testnet-only; no mainnet path. Keys are only touched in `aen-chain` (`derive_signer` is called once in `server::chain::from_env` for the dev test owner, and only when `AEN_DEV_CHAIN_OWNER=1`).
