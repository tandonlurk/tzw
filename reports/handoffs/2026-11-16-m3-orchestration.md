# Handoff 2026-11-16 — M3 orchestration (lead)

Living file. If the session died, trust `git` over this file. Resume: `tasks/project.md` "Resume procedure".

## Verified baseline (recovery @ 258e8a1)
Lead ran: backend `cargo test --workspace` 254, contracts `forge test` 84, web `npm test` 30.

## Specialists (background subagents, one worktree + branch each from `recovery`)

| Name | Worktree | Branch | Owns | Dbs / ports | Status |
|---|---|---|---|---|---|
| backend-m3 | `~/dev/hackathongs/worktrees/backend-m3` | `backend-m3` | `backend/`, `schemas/`, `docs/api.md`, `tasks/backend.md` | `aen_m3_test`, `aen_m3_sec`, `aen_m3_dev`, port 8093 | stage 1 running |
| sandbox-spike | `~/dev/hackathongs/worktrees/sandbox-spike` | `sandbox-spike` | `spikes/sandbox/`, `docs/research/sandbox-spike.md`, `tools/` (gitignored) | none | running |
| web-m3 | `~/dev/hackathongs/worktrees/web-m3` | `web-m3` | `web/`, `tasks/frontend.md`, `reports/qa-m2-web.md` | `aen_w3`, ports 8094 / 3013 | waits for backend-m3 stage 1 |

Cargo target dirs: `~/dev/hackathongs/.cargo-target/<name>`.

backend-m3 stages: 1 = proposal rate-limit for denied/no-counterparty + `pnl_over_time` uses settlement time; deterministic demo step yielding a rule-generated suggestion with a change. 2 = SEC-02/08/09, then collaboration MVP, then local Streams webhook ingestion (one report per item).

## Lead work
- `docs/submission/` drafts (one per award) and root `README.md` written; refresh after each merge.

## Merge log
