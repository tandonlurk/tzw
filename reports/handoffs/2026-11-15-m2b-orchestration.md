# Handoff 2026-11-15 — M2b orchestration (lead)

Living file. If the session died, trust `git` over this file. Resume steps: same as `2026-11-15-m2-orchestration.md` ("How to resume").

## Verified baseline (recovery @ b098cad)
Lead ran: backend `cargo test --workspace` 211 pass, contracts `forge test` 80 pass, web `npm test` 21 pass.

## Specialists (background subagents, one worktree + branch each, all from b098cad)

| Name | Worktree | Branch | Owns | Dbs / ports | Status |
|---|---|---|---|---|---|
| backend-m2b | `~/dev/hackathongs/worktrees/backend-m2b` | `backend-m2b` | `backend/`, `schemas/`, `docs/api.md`, `tasks/backend.md` | `aen_m2b_test`, `aen_m2b_sec`, `aen_m2b_dev`, port 8091 | running |
| web-m2b | `~/dev/hackathongs/worktrees/web-m2b` | `web-m2b` | `web/`, `tasks/frontend.md`, `reports/qa-m2-web.md` | `aen_qa`, ports 8090 / 3010 | running |
| contracts-sec | `~/dev/hackathongs/worktrees/contracts-sec` | `contracts-sec` | `contracts/`, `tasks/contracts.md` | none | running |

Cargo target dirs: `~/dev/hackathongs/.cargo-target/<name>`.

Scope: backend = proposal cooldown, readable negotiation summaries, approval-conviction and clock-jump root causes, `schemas/` for challenge terms, demo seed + lifecycle test. Web = browser-verify reject / approve-then-cancelled / agent-detail challenges / template prefill, Next dev "1 Issue" badge, per-agent P&L lines. Contracts = SEC-04/SEC-07 cap `resolveDeadline`, local only.

## After each merge
Review diff vs AGENTS.md section 8, run its tests, `git merge --no-ff`, run all three suites, copy "New defaults" from the task file into `docs/decisions.md`, then `git worktree remove`, delete the cargo target dir, `dropdb` its dbs.

## Merge log
(none yet)
