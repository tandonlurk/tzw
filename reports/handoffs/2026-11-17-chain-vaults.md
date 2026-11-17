# Handoff 2026-11-17 — onchain escrow + agent vaults (lead)

Trust `git` over this file. Resume: `tasks/project.md` "Resume procedure".

## State

- Branch `chain-vaults`, draft PR https://github.com/tandonlurk/tzw/pull/1 (base `main`). `main` = old `recovery` tip `d721f87`.
- Owner decisions (2026-11-16): custody = per-agent `AgentVault` owned by the owner's **browser wallet** (D-57); vision items (optional TZW-held vault with a user benefit, pluggable custody providers, minimal platform control/privacy) in `TODO.md` "Vision".
- Defaults taken (not yet in decisions.md, record when implemented): fake market data for automated chain tests, real Gemini for live runs; confirm onchain effects from tx receipts / escrow reads now, QuickNode Streams later as the event/audit source (revises D-16).
- Done and tested: `contracts/src/AgentVault.sol` (vault + factory, `forge test` 100 pass); `backend/crates/chain` (`aen-chain`, alloy 2.6) with anvil lifecycle test (`cargo test -p aen-chain`, needs `forge build` output). Server does not use `aen-chain` yet.
- Keys: testnet-only seed in `backend/.env` (D-58). idx 0 resolver/deployer `0x599F37a9815e13319E2C3AA04b850876706c4C90`, idx 1 operator `0x84dDb2795697e6EE319fe9bb55b5D5c1c3B90eca`, idx 2 test owner `0x1F0eb73C714Ae0BD1a97c1c8e0F34513073393Fb`.
- **Blocked on owner:** faucet funds (`docs/setup/arc-testnet-funding.md`). Nothing deployed. No background processes running.

## System now

```
                         ┌───────────────────────────────┐
                         │  Owner                        │
                         │  Web app (Next.js, :3000)     │
                         └──────────────┬────────────────┘
                                        │ REST /api
                ┌───────────────────────▼────────────────────────┐
                │  aen-server (Rust/axum, :8080)                 │
                │  tick: discover → negotiate → authorize()      │
                │        → commit → active → resolve → settle    │
                │  ├ graph + custom code ─► aen-sandbox (Wasm)   │
                │  ├ reasoner: rule-based ✅  Grok ⚠ mock only   │
                │  ├ co-captain ✅  collaborations ✅ (simulated)│
                │  └ QuickNode webhook ⚠ local only, unverified  │
                └───┬──────────────┬─────────────────┬───────────┘
                    │              │                 ┆ ❌ not wired yet
          ┌─────────▼───┐  ┌───────▼────────┐  ┌─────▼──────────────────┐
          │ PostgreSQL  │  │ Market data    │  │ aen-chain (alloy) ✅   │
          │ ledger      │  │ Gemini ✅      │  │ resolver key (idx 0)   │
          │ (simulated) │  │ fake feed ✅   │  │ operator key (idx 1)   │
          └─────────────┘  └────────────────┘  └─────┬──────────────────┘
                                                     │ anvil ✅ · Arc ❌
   ┌─────────────────────────── Arc testnet (EVM) ───▼────────────────────┐
   │  Owner MetaMask ──allocate──► AgentVault A ──stake──┐                │
   │   (UI ❌)  owns: pause,       AgentVault B ──stake──┤                │
   │   limits, withdraw, operator       ▲                ▼                │
   │                                    └── payout ── ChallengeEscrow     │
   │   VaultFactory creates vaults            create/resolve/void         │
   │   ✅ written, 100 Foundry tests          (resolver only)  ❌ not deployed │
   └──────────────────────────────────────────────────────────────────────┘
          QuickNode Streams ┆ ❌ no stream yet (would watch these events)
```

## Next steps (in order)

1. **Deploy to Arc testnet** once idx 0 is funded: `ChallengeEscrow` + `VaultFactory` via `contracts/script/Deploy.s.sol` (stock forge, `--with-gas-price 20gwei` floor; arc-forge if stock fails), verify on Blockscout, record addresses in `docs/decisions.md` + `backend/.env.example` (addresses only). One manual create → stake → resolve smoke run with the test owner. Note any Arc-vs-anvil difference (evm_version, blocklist revert).
2. **Wire chain into the lifecycle** (`backend/crates/server`):
   - core ledger: `Reason::VaultDeposit/VaultWithdraw` (external ↔ agent available, env `arc_testnet`), keys `chain:{tx}:{log_index}`.
   - migration 0009: `agents.vault_address/vault_owner`, interaction chain tx hashes.
   - settings: `ARC_RPC_URL`, `ARC_ESCROW_ADDRESS`, `ARC_VAULT_FACTORY_ADDRESS`, `AEN_TESTNET_MNEMONIC`; `AppState.chain: Option<Chain>`.
   - `try_commit` for `arc_testnet`: same authorize + ledger reservation, state `funded` (not `active`).
   - new tick phase for `funded` + arc: read escrow → `create` if none → `vault_stake` each missing party → `active` when escrow Active; void + ledger refund + cancel on failure or funding deadline.
   - resolve for arc: `resolve(winner vault)` / `resolveVoid` before ledger settle, then `vault_release` both. Retry next tick on RPC failure.
   - arc interactions use the wall clock (ignore the dev tick's simulated `now`; never resolve before `end_ts`).
3. **Endpoints:** `GET /api/chain` (config for the web); `POST /api/agents/{id}/vault {tx_hash}` (verify `VaultCreated` from our factory, `agentRef` = agent, operator = ours; switch agent to `arc_testnet`; refuse with open interactions); `POST /api/agents/{id}/vault/sync {tx_hash}` (USDC transfers in/out of the vault → ledger, idempotent); `GET /api/agents/{id}/vault` (onchain balance/exposure/caps/paused vs ledger, drift).
4. **Autonomous runs:** dev-only endpoint (env-gated) using the test-owner key to create + fund vaults for two agents; run with `TICK_INTERVAL_SECS` set and short horizons (~2 min funding, 5–10 min challenge).
5. **Server E2E test on anvil:** two vault agents, fake market, ticks until settled; ledger equals onchain balances.
6. **Browser wallet UI** (viem): connect + switch to Arc, create vault per agent, allocate (amount or %), withdraw, pause, live balances + explorer links.
7. **Live Arc demo run** with real Gemini data; update `docs/demo.md`, `docs/submission/arc.md`, main-track write-up.
8. Then: QuickNode Streams on escrow/vault events (owner account + tunnel), Grok key, ElevenLabs.

Estimate: steps 2–5 about a day, step 6 about half a day.
