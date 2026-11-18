# Handoff 2026-11-18 — approach block, firms, live Gemini testnet run (lead)

Trust `git` over this file. Design: `docs/design/approach-and-firms.md`. Decisions D-64 (backend), D-65 (owner interpretation + web).

## Merged into `chain-vaults`
- `testnet-gemini` (d9147b3): live Arc run on real Gemini data, docs/demo.md "Testnet (Arc) demo", submission status.
- `approach-firms-web` (bdc8e34): approach editor (stance + market wheels), pinned Approach block, Firms list/detail/create, nav, activity kinds.
- `approach-firms-backend` (7b90e0d): `aen_core::{approach, firm}`, `PayTaskFee` intent, migration 0010, firms API + runtime, `/markets/categories`, `/markets/preview`, `TickReport.activated`.

## Verified on the merged branch
- `cargo test --workspace`: 388 passed, 0 failed, 1 ignored; clippy clean. Web: 112 tests, lint 0 errors, build OK.
- HTTP E2E on a local test server (fake market, db `aen_firm_demo`): lead + scout + analyst + skeptic; firm 80/20, fee 0.10.
  Tick 1 proposed + 2 contributions; tick 2 accepted + approval (approved); tick 3 active; jump past end → settled.
  Lead +5.00 → paid 1.00 pool (scout 0.60, analyst 0.40 by 60/40 weights) + 2 × 0.10 fees → lead net +3.80.
- Real web app against that backend (Chrome): firm detail (stats, split bar, roles), approach editor loads the lead's real params, derived numbers correct.
- Live Gemini (read-only test server): categories Commodities/Crypto/Sports/Weather with subcategories (Oil (WTI), Gold, …); preview: any 1006, Commodities 24, keyword "oil" 6, BTC up/down 17.
- Script used: run a server on a spare port with `DEV_ENDPOINTS=true MARKET_DATA=fake TICK_INTERVAL_SECS=0`, then create agents from templates with an `approach.profile` node appended (lead: collab 5000 / ambition 10000 / "lead"; members: 8000 / 5000 / "member"), `POST /firms`, `/firms/{id}/join` ×2, `/dev/fake-market {"move_bps":300,"yes_price_bps":5500,"resolution_side":"open"}`, 3 × `/dev/tick` (approve the pending approval), `/dev/fake-market {"yes_price_bps":7000}`, `/dev/tick {"now": end_at + 60 s}`.

## Gaps (in TODO M3b)
- Fake market mode has one market, so the market wheel shows only Crypto → Bitcoin offline.
- `firm_challenge_pays_fees_and_shares_the_profit` test forces the interaction state via SQL; the HTTP run above shows the real path works.
- Testnet run gaps: vault release tx not recorded; `/stats/portfolio` simulated-only; discovery threshold from stale `lastTradePrice`.
- No firm demo seed endpoint; starter templates have no approach node; firms not on Arc.

## Housekeeping
- Disk filled once (ENOSPC) with six worktrees each building `target/`. Agents now share `CARGO_TARGET_DIR=<main>/backend/target`. Free ~20 GB at hand-off.
- Test dbs left: `aen_arc_demo`, `aen_arc_gemini`, `aen_firm_demo`, `aen_test_firms`, `aen_sec_test_firms`, `aen_*_wiring`.
