# Handoff 2026-11-15 — M2 orchestration (lead)

Living file: the lead updates it as branches report. If the session died, trust `git` over this file.

## Verified baseline (recovery @ 2a6c7ed)
- `cd backend && cargo test --workspace`: 101 pass. `cd contracts && forge test`: 72 pass. Web not yet verified (qa-web does it).

## Specialists launched (background subagents, one worktree + branch each, all from 2a6c7ed)

| Name | Worktree | Branch | Owns | Task file | Status |
|---|---|---|---|---|---|
| qa-web | `~/dev/hackathongs/worktrees/qa-web` | `qa-web` | `web/`, `reports/qa-m1.md` | `tasks/frontend.md` | MERGED, worktree removed |
| backend-m2 | `~/dev/hackathongs/worktrees/backend-m2` | `backend-m2` | `backend/`, `schemas/`, `docs/api.md`, `docs/research/gemini-prediction-markets.md` | `tasks/backend.md` | MERGED, worktree removed |
| security-tests | `~/dev/hackathongs/worktrees/security-tests` | `security-tests` | `docs/security/`, new test files, `reports/security-m1.md` | `tasks/qa-security.md` | MERGED, worktree removed |

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
- security-tests merged into `recovery`. Lead re-ran: `cargo test --workspace` 136 pass + 1 ignored (SEC-01 repro); `forge test` 80 pass. Adversarial server tests need db `aen_sec_test` (`createdb aen_sec_test`). Findings in `reports/security-m1.md`. SEC-01 (CSRF via text/plain POST), SEC-03 (double settle across interactions), SEC-05 (413) and the dotted 422 path doc were SENT to backend-m2 to fix on its branch: verify they are in its final report before merging. Still open, owner/contract work: SEC-04 + SEC-07 (cap `resolveDeadline`, before first Arc deploy), SEC-02 (auth/Host check before any tunnel).
- backend-m2 merged into `recovery`. Lead re-ran after merge: `cargo fmt --check` ok, clippy 0 warnings, `cargo test --workspace` 211 pass / 0 ignored, `forge test` 80, web `npm test` 15. SEC-01/03/05 fixed. Financial defaults copied to `docs/decisions.md` D-22..D-31. Known gaps: no proposal cooldown (agents re-propose every tick); Grok endpoint/model unverified; only `resolutionSide: yes` seen live; `schemas/` not extended for challenge terms; all POSTs need a JSON body (`{}`).
- NEXT: `frontend-m2` (worktree `~/dev/hackathongs/worktrees/frontend-m2`, branch `frontend-m2`, owns `web/`). If it is not merged: resume per steps 1-2 above. After it: backend follow-ups (proposal cooldown, schemas for terms), fill `docs/demo.md` from `backend/README.md`, AGENTS.md 9.4 demo line.
(lead appends here)
