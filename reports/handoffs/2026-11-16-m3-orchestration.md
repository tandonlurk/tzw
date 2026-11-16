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
