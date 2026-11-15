# Handoff 2026-11-15 — M2 orchestration (lead)

Living file: the lead updates it as branches report. If the session died, trust `git` over this file.

## Verified baseline (recovery @ 2a6c7ed)
- `cd backend && cargo test --workspace`: 101 pass. `cd contracts && forge test`: 72 pass. Web not yet verified (qa-web does it).

## Specialists launched (background subagents, one worktree + branch each, all from 2a6c7ed)

| Name | Worktree | Branch | Owns | Task file | Status |
|---|---|---|---|---|---|
| qa-web | `~/dev/hackathongs/worktrees/qa-web` | `qa-web` | `web/`, `reports/qa-m1.md` | `tasks/frontend.md` | MERGED, worktree removed |
| backend-m2 | `~/dev/hackathongs/worktrees/backend-m2` | `backend-m2` | `backend/`, `schemas/`, `docs/api.md`, `docs/research/gemini-prediction-markets.md` | `tasks/backend.md` | running |
| security-tests | `~/dev/hackathongs/worktrees/security-tests` | `security-tests` | `docs/security/`, new test files, `reports/security-m1.md` | `tasks/qa-security.md` | running |

Each commits after every working step and logs progress in its task file ON ITS BRANCH. Cargo target dirs: `~/dev/hackathongs/.cargo-target/<name>`. Dbs: `aen_qa`, `aen_m2_dev`, `aen_m2_test`, `aen_sec_test`. Ports: qa backend 8090, qa web 3010.

## How to resume if the session was cut off
1. `git worktree list && git branch -a`; for each branch: `git log --oneline recovery..<branch>` and read its task file in the worktree (`tail -30`).
2. A branch with commits but no final report: check `git status` in its worktree for uncommitted work, run its tests, then either re-spawn a specialist in the SAME worktree with "continue from tasks/<file>.md next steps" or merge what is green.
3. Kill leftovers: `lsof -i :8090 -i :3010`.
4. Merge order: security-tests (new files only) -> qa-web (`web/` only) -> backend-m2. For each: review diff vs `docs/architecture.md` + AGENTS.md section 8, run its tests, `git merge --no-ff <branch>` into `recovery`, then full suites (backend, contracts, web).
5. After merging: `git worktree remove ~/dev/hackathongs/worktrees/<name>`, `rm -rf ~/dev/hackathongs/.cargo-target/<name>`, `dropdb` the specialist db.
6. Then: route qa-web backend bugs and security findings to backend tasks (highest severity first); once backend M2 API is merged, spawn `frontend-m2` (approvals queue, interactions list/detail with negotiation + trace, richer activity, dashboard charts only from `/stats/portfolio`, empty states).
7. Copy new financial defaults from `tasks/backend.md` into `docs/decisions.md` (status `default`, flagged for review). Update `TODO.md` lines (edit, never rewrite) and `tasks/project.md`.

## Not allowed without the owner
D-3 custody/signing, any chain deploy or transaction, real API keys, mainnet, track/bounty changes, anything under "Needs you" in `TODO.md`. No push unless asked.

## Merge log
- qa-web merged into `recovery`. Lead re-ran in `web/`: `npm test` 15 pass, `npm run lint` 0 errors / 2 warnings, `npm run build` ok. No backend bugs found. Left over (for frontend-m2): browser-test add block / connect / incompatible-connection rejection (unit-tested only); prefill template description in create form; document dotted 422 `details[].path` in `docs/api.md` (backend-m2 owns that file, do after its merge).
(lead appends here)
