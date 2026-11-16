# Project tracker

Lead-owned. `TODO.md` is the user-facing list; this file holds status detail, risks and the resume procedure.

## Resume procedure (new session, any model)

1. Read `AGENTS.md`, `TODO.md`, this file, the latest file in `reports/handoffs/`, then only the task file you are working on.
2. Run `git status && git log --oneline -10 && git branch -a` and trust that over any report.
3. Read `docs/architecture.md` and `docs/decisions.md` before changing interfaces. Read `docs/research/*` instead of re-researching.
4. After each meaningful unit of work: update the task file, tick `TODO.md` (edit lines, never rewrite the file), commit on the specialist branch.
5. Before stopping: write `reports/handoffs/YYYY-MM-DD-<topic>.md` with exact next actions.

Token discipline: read sections, not whole files; delegate web research and bulk implementation to subagents (cheaper model for routine work) with reports capped at ~200 words; keep facts in `docs/`, not in conversation.

## Status (2026-11-15, Sun)

- Phase: **M2 simulated lifecycle done and hand-verified (backend 222, contracts 84, web 25).** Chain work blocked on D-3. Live state: `reports/handoffs/2026-11-15-m2b-orchestration.md`.
- Default branch `recovery`; baseline backend 101 / contracts 72 tests pass.

## Status (2026-11-14, Sat) — historical

- Phase: **M0 planning. Awaiting owner approval before implementation.**
- Repo: empty apart from `AGENTS.md`. Branch `planning` holds the planning docs.
- Machine: Node 22 + npm + Homebrew present. **Missing: Rust, Foundry, PostgreSQL, Docker, pnpm.**
- No API keys or `.env` present.

## Milestones

| M | Target | Scope | Status |
|---|---|---|---|
| M0 | Sat 14 | Research, plan, approval, toolchain, scaffold | in progress |
| M1 | Sun 15 | core (ledger, permissions, graph schema), DB, agent CRUD, escrow contract + tests, web shell + agent creation | not started |
| M2 | Mon 16 | runtime, Gemini data, reasoner, negotiation, approvals, Arc deploy, visual builder | not started |
| M3 | Tue 17 | Streams ingestion, full testnet lifecycle, wallet/activity/dashboard, collaboration | not started |
| M4 | Tue–Wed | QA + security, continuous | not started |
| M5 | Tue eve–Wed 18 | Custom-code sandbox | not started |
| M6 | Wed 18 | Co-captain, voice, demo prep. Demo working EOD | not started |
| — | Thu 19 | Buffer and submission | — |

Cut order if behind: ElevenLabs → collaboration → co-captain → custom code reduced to a minimal sandboxed block (not removed). The single challenge lifecycle is never cut.

## Delegation

| Specialist | Branch | Task file | Owns |
|---|---|---|---|
| Research | `planning` | `tasks/research.md` | `docs/research/` |
| Contracts | `contracts` | `tasks/contracts.md` | `contracts/` |
| Backend | `backend` | `tasks/backend.md` | `backend/`, `schemas/` |
| Frontend | `frontend` | `tasks/frontend.md` | `web/` |
| QA/Security | `qa` | `tasks/qa-security.md` | tests across modules, `docs/security/` |
| Lead | `main` merges | this file | `TODO.md`, `docs/architecture.md`, `docs/decisions.md`, root config |

Interfaces between specialists: the REST API in `docs/architecture.md` §9, the JSON Schemas in `schemas/`, and the contract ABI in `contracts/out`. Backend defines the first two before frontend builds against them; frontend may start on the shell and static schema in parallel.

## Risk register

| # | Risk | Impact | Mitigation |
|---|---|---|---|
| R1 | Scope: Rust backend + builder + sandbox + 5 sponsors in 5 days | Unfinished demo | Lifecycle first; cut order above; daily check against milestones |
| R2 | Key custody for agent wallets | Fund loss (testnet only), bad precedent | Testnet-only seed in env, isolated signer, owner approval (D-3), no mainnet config exists |
| R3 | LLM output treated as authority | Unauthorized spend | Typed `Intent` + schema validation + pure `authorize()`; tests for over-limit and malformed output |
| R4 | Resolution relies on an exchange API, not an oracle | Wrong or disputed outcome | Explicit terms and formula, labelled backend-administered, timeout refund path |
| R5 | Double counting in P&L | Wrong numbers on screen | Double-entry ledger, formulas in architecture §6, invariant tests (sum of entries = 0) |
| R6 | Streams may not support Arc, or duplicate/reorged events | Missed or duplicated settlement | Idempotency on tx hash + log index; polling fallback via RPC |
| R7 | Arc testnet quirks (USDC as gas, decimals) | Deploy or transfer failures | Verify in research before writing chain code; Foundry fork tests |
| R8 | Custom code escapes or exhausts resources | Backend compromise or DoS | Wasm sandbox, fuel/epoch + memory limits, no WASI caps, output validated |
| R9 | Webhook endpoint spoofing | Forged settlement events | Verify Streams signature; cross-check tx receipt via RPC before applying |
| R10 | Session/context loss mid-week | Rework | This file, handoffs, small commits |
| R11 | Bounty criteria unverified for this event | Integrations that do not qualify | Owner to share bounty page; research records what is and is not verified |

## Log

- 2026-11-14: Kickoff. Inspected repo and machine. Launched two research tasks. Wrote architecture, decisions, TODO, trackers.
- 2026-11-15: Recovery from iCloud data loss (see handoff). Verified baseline; launched qa-web, backend-m2, security-tests in `~/dev/hackathongs/worktrees/`.
- 2026-11-15: Merged qa-web, security-tests, backend-m2 into `recovery` (backend 211, contracts 80, web 15 tests). Simulated challenge lifecycle works. Spawning frontend-m2.
- 2026-11-15: Merged frontend-m2 (web 21 tests). All worktrees removed. Next actions in `reports/handoffs/2026-11-15-m2-orchestration.md`.
