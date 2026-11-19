# Handoff 2026-11-19 — agent multiplier (1x / 10x / 100x)

Branch `worktree-agent-a14bf58cb0a97c345`. Decision D-73. Simulated funds only.

## What changed
- `POST /api/agents/{id}/multiply` `{count 1-100, variance_bps 0-2500, budget, seed?, dry_run?}` (`backend/crates/server/src/multiply.rs`, migration `0013_agent_families.sql`: append-only `agent_multiplications` and `agent_clones`). Spec: `docs/api.md` "Agent multiplier".
- Money: budget split evenly (floor), remainder stays unallocated; one `allocate` posting set per clone from `user:unallocated`, all in one transaction. Shortfall = 422 `insufficient_funds`, nothing written. Family cap 200 clones (409 `family_full`); paused source 409; non-simulated source 422.
- Variance: seeded, deterministic per (seed, clone index, parameter path); strategy parameters only (horizon, market-move threshold/window, schedule, conviction gate, approach ambition). Money limits copied; `policy.min_conviction_bps` only rises; a server check refuses any looser policy. Each clone's draws are stored and returned as `Agent.clone.params`.
- `Agent` gains `family_id` and `clone`. `lifecycle::pick_counterparty` skips siblings (same family) and, in SQL, agents with an open interaction on the market (was N `load_agent` calls; matters with 100+ agents).
- Web: Multiply card on the agent overview (presets 1x/10x/100x + custom, variance slider, budget, client-side split preview, then server dry run table, confirm with the same seed); header "Multiply" button and "Clone #n" badge; Family card on agent pages (source, seed, varied params, family totals); My Agents shows clones under a Families section (family P&L, available, committed, record, expandable clone table) instead of as cards. Pure logic in `web/lib/multiply.ts`.
- Docs: D-73, `docs/api.md`, `backend/README.md` "Multiplier demo".

## Tests
- `cargo test --workspace`: 403 passed, 2 ignored, 0 failed (new: `tests/multiply.rs` 5, `multiply` unit 4). Chain tests need `contracts/out`; this fresh worktree had none, so it was copied from the main checkout (gitignored). Clippy clean on `aen-server`.
- Web: `npm test` 161 passed in 24 files (new `lib/multiply.test.ts`: 7), `npm run build` OK, `npm run lint` 0 errors (1 pre-existing warning in `lib/graph.test.ts`).

## E2E (db `aen_mult`, port 8090, fake market, `TICK_INTERVAL_SECS=0`)
- Seed demo + deposit 300: unallocated 300, allocated 200. Bull 10x and skeptic 10x, 100 USDC each at 10 % variance: unallocated 100, allocated 400, 22 agents, sum of available 400. Dry run with seed 4242 predicted the real clones exactly.
- 100x with 200 budget refused (`insufficient_funds`, nothing written); count 101 refused (422 `count`).
- Tick 1: proposed 10, countered 9, accepted 1, approvals 1. Tick 2: accepted 5, approvals 5. 6 approvals approved, all active. 0 same-family pairs. 4 cancelled: bull clones whose raised conviction floor rejected the skeptic's 2.50 counter.
- Resolve at YES 0.70: settled 6, voided 0. Bull family +17.50 (6 wins), skeptic family -17.50, portfolio realized 0, committed 0, unallocated 100, allocated 400. Ledger balanced.

## Not done / for review
- No `Idempotency-Key` on multiply (a double submit funds a second batch correctly). No bulk "approve all" for the approvals a 10x family generates.
- Web UI not clicked through in a browser (build, lint and unit tests only).
- Family combined exposure can reach clones × per-agent limit, bounded only by the capital allocated (documented in D-73). Owner may prefer a per-family exposure cap.
- No delete / retire for clones (pause works per agent).
