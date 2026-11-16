# Handoff 2026-11-16 — M3 orchestration (lead)

Living file. If the session died, trust `git` over this file. Resume: `tasks/project.md` "Resume procedure".

## Verified baseline (recovery @ 258e8a1)
Lead ran: backend `cargo test --workspace` 254, contracts `forge test` 84, web `npm test` 30.

## Specialists (background subagents, one worktree + branch each from `recovery`)

| Name | Worktree | Branch | Owns | Dbs / ports | Status |
|---|---|---|---|---|---|
| backend-m3 | `~/dev/hackathongs/worktrees/backend-m3` | `backend-m3` | `backend/`, `schemas/`, `docs/api.md`, `tasks/backend.md` | `aen_m3_test`, `aen_m3_sec`, `aen_m3_dev`, port 8093 | stage 1 running |
| sandbox-spike | `~/dev/hackathongs/worktrees/sandbox-spike` | `sandbox-spike` | `spikes/sandbox/`, `docs/research/sandbox-spike.md`, `tools/` (gitignored) | none | running |
| web-m3 | `~/dev/hackathongs/worktrees/web-m3` | `web-m3` | `web/`, `tasks/frontend.md`, `reports/qa-m2-web.md` | `aen_w3`, ports 8094 / 3013 | stage 1 (agent detail/editing, no backend dependency) running; stage 2 after backend-m3 stage 1 merges |

Cargo target dirs: `~/dev/hackathongs/.cargo-target/<name>`.

backend-m3 stages: 1 = proposal rate-limit for denied/no-counterparty + `pnl_over_time` uses settlement time; deterministic demo step yielding a rule-generated suggestion with a change. 2 = SEC-02/08/09, then collaboration MVP, then local Streams webhook ingestion (one report per item).

## Lead work
- `docs/submission/` drafts (one per award) and root `README.md` written; refresh after each merge.

## Merge log
- web-m3 stage 1 merged (c885a3e). Lead re-ran after merge: web 33 (specialist reported 36; actual is 30 + 3), lint 0 errors / 2 warnings, build ok; backend 254; contracts 84. Agent detail now has version history with diffs, per-agent activity, unrealized P&L "n/a, not computed", win rate, "Saved as version N" notice, "(config vN)" on decision traces. NOT browser-verified yet (stage 2 Chrome pass covers it). Worktree kept for stage 2.
- backend-m3 stage 1 merged (2390662, 76d8c0e). Lead re-ran after merge: fmt ok, clippy 0 warnings, backend 256, contracts 84, web 33. Migration 0005 (`agent_runs.market_key/blocked`, `interactions.settled_at`). README demo gains two rounds: skeptic's third loss → rule-generated `lower_max_stake` (max_per_interaction 5.00) plus `raise_approval_threshold`; accept → version 2. D-38/D-39 copied to decisions. Worktree kept for stage 2.
- sandbox-spike merged (c1d3f79): **GO for D-11**. QuickJS (Javy v9.1.0, SHA-256 verified) in Wasmtime 50.0.1; fuel, epoch deadline, 32 MiB cap, no WASI caps, strict output. Lead re-ran spike `cargo test`: 17 passed, 1 ignored (latency); then backend 256, contracts 84, web 33. Whole crate merged (standalone `spikes/sandbox/`, not in the backend workspace). Javy CLI in gitignored `tools/`, built `runner.wasm` gitignored. Worktree and target dir removed. D-40/D-41 added.
- sandbox-host started (worktree `~/dev/hackathongs/worktrees/sandbox-host`, branch `sandbox-host`, no db): stage 1 = `backend/crates/sandbox` production crate + escape/abuse tests; stage 2 = `custom.code` runtime block after backend-m3 finishes.
