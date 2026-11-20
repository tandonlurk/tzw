# Project tracker

Lead-owned. `TODO.md` is the user-facing list; this file holds status detail, risks and the resume procedure.

## Resume procedure (new session, any model)

1. Read `AGENTS.md`, `TODO.md`, this file, the latest file in `reports/handoffs/`, then only the task file you are working on.
2. Run `git status && git log --oneline -10 && git branch -a` and trust that over any report.
3. Read `docs/architecture.md` and `docs/decisions.md` before changing interfaces. Read `docs/research/*` instead of re-researching.
4. After each meaningful unit of work: update the task file, tick `TODO.md` (edit lines, never rewrite the file), commit on the specialist branch.
5. Before stopping: write `reports/handoffs/YYYY-MM-DD-<topic>.md` with exact next actions.

## Active work (2026-11-20) — Grok Bot mock adversary

### Demo portfolio and forum follow-up (owner request)

- Integrated forum UI `371f1c8`, API-only portfolio seeder `2c4310a`, forum backend `1d020f1`. Backend forum tests 3/3 passed; UI 168 tests, TypeScript, targeted lint and production build passed. Lead reviewed module/API compatibility and executed the seed through HTTP.
- Current test server: web `http://127.0.0.1:3000`, backend `http://127.0.0.1:8090/api`, isolated DB `aen_video_demo_20261120`; existing `aen_dev` preserved. Simulated scheduler every 15 seconds; Arc signing and xAI model key disabled for this process. Older 8080 backend stopped. Running backend binary `/private/tmp/tzw-forum-target/debug/aen-server`, explicit sandbox path to root runner.
- Verified capture: 6000 simulated USDC, 1500 unallocated, 218 committed, three roots (Atlas 2×, Vega 3×, Lumen 5×; ten agents including clones), 36 settled/3 active challenges, 22 wallet/274 agent-ledger entries. Six labeled demo forum threads/replies seeded; all relevant pages and proxy data verified HTTP 200. Counts may advance with scheduler.
- Forum supports owner-authored persistent threads/replies and categories; local prototype, not public multiuser hosting. Firms/Challenges descriptions clarified. No real activity/performance claims. Reproduce: `docs/demo-portfolio.md`.
- Another session has unrelated uncommitted dashboard/navigation changes plus video edits in the shared checkout; preserve them, stage only our exact documentation files.

- Owner confirms PR #10 merged; another agent owns `demo-video/` and is actively committing there. Do not edit its files or switch this shared checkout away from `integration-1119`.
- Owner selected Grok Bot mock adversary (D-75). Lead owns setup/API docs, decisions, tracker and integration. Backend specialist owns backend in isolated worktree; UI specialist owns web in isolated worktree. API contract: owner `/agents/{id}/sparring`, `/sparring/{id}`, `/sparring/{id}/rounds`; restricted Bot `/bot/sparring/{id}` and `/bot/sparring/{id}/rounds`.
- Practice session and rounds only: no financial effects, no automatic configuration changes, no live negotiation gate. Bot uses a separate token; live Bot use remains unverified until owner signs in and runs a real critique.
- Local implementation integrated: backend `8c38667`, UI `84a5058`. Independent static review has no unresolved high/medium findings; polling draft-loss bug fixed. Backend library 84 passed / 1 ignored, four isolated PostgreSQL integration tests passed; web 168 tests, TypeScript, targeted lint and production webpack build passed. Shared checkout diff check clean. Setup/handoff: `docs/setup/grok-bot-sparring.md`, `reports/handoffs/2026-11-20-grok-adversary.md`.
- Owner direction (2026-11-20): do not buy Grok Bot; keep the sparring infrastructure as a prototype and describe the Bot connection as a future integration at the end of the demo. Live Bot setup is deferred, not a submission blocker. No automated adversary reasoning or live Bot execution is claimed. No pushes; video agent files preserved.

Token discipline: read sections, not whole files; delegate web research and bulk implementation to subagents (cheaper model for routine work) with reports capped at ~200 words; keep facts in `docs/`, not in conversation.

## Status (2026-11-17, Tue evening) — current

- Phase: **Arc testnet live + approach block + firms done; owner reviewing.** Stacked PRs, merge in order: #1 `chain-vaults`→`main`, #2 `arc-lifecycle`, #3 `approach-firms`. After the owner merges, sync with `git fetch && git checkout main && git pull` before starting work. Done: contracts deployed and verified on Arc (D-60/61), chain wired into the lifecycle (D-63), wallet UI (D-62, not yet tried with MetaMask), live Arc run on real Gemini data settled with drift 0, approach block + firms (D-64/65). Tests: backend 388 (1 ignored), web 112. Live state and next actions: `reports/handoffs/2026-11-18-approach-firms.md`; open items: `TODO.md` "M3b" and "Notes from owner".
- Review database `aen_review` (copy of the firm demo) exists for re-running the review server; no server is running.

## Status (2026-11-17, Tue morning) — historical

- Phase: **onchain custody started (PR #1, branch `chain-vaults`).** D-57 owner decision: per-agent `AgentVault`s owned by the owner's browser wallet. Done: vault + factory contracts (contracts 100 tests), `aen-chain` crate with anvil lifecycle test. Not done: Arc deploy (blocked on faucet funds, owner action in `TODO.md` "Start here"), lifecycle wiring, vault endpoints, autonomous-run tooling, wallet UI. Plan + diagram: `reports/handoffs/2026-11-17-chain-vaults.md`.

## Status (2026-11-16, Mon) — historical

- Phase: **M3/M5 done on simulated funds.** Merged today: blocked-proposal cooldown, settlement-time P&L, deterministic co-captain demo step, SEC-02/08/09, collaborations (backend + web), local QuickNode Streams webhook, `aen-sandbox` + `custom.code` block + code editor, agent detail/version history, dashboard suggestions banner. Tests: backend 332 (1 ignored), contracts 84, web 45. Full README demo hand-verified (`docs/demo.md`). Live state: `reports/handoffs/2026-11-16-m3-orchestration.md`.
- Everything chain-side is blocked on D-3 (custody) + faucet funds; Grok live use on the xAI key + D-8; Streams on a QuickNode account + tunnel decision.

## Status (2026-11-15, Sun) — historical

- Phase: **M2 simulated lifecycle done and hand-verified (backend 254, contracts 84, web 30); co-captain backend and page merged.** Chain work blocked on D-3. Live state: `reports/handoffs/2026-11-15-m2b-orchestration.md`.
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
- 2026-11-16: Lead ran backend-m3 (5 items), web-m3 (5 stages), sandbox-spike (D-11 GO), sandbox-host (crate + custom.code). All merged into `recovery`, all worktrees, target dirs and specialist dbs removed. Submission drafts in `docs/submission/`, root README added.

## 2026-11-20 — owner-requested landing page and submission copy
- Devpost paragraphs saved separately beside narration: `demo-video/devpost-responses.md`; existing voiceover script preserved.
- Default `/` now introduces TZW with owner's logo rendered in white ASCII on black, stationary hands and a rotating coin. Get started opens the preserved overview at `/dashboard`; shell links and presentation-mode mapping updated.
- Frontend tests (168), TypeScript, targeted lint, production build pass; desktop/mobile browser checks verify no overflow, stationary hands/animated coin, reduced-motion support, and direct dashboard rendering. Detail: `tasks/frontend.md`.
- No sponsor, custody, financial, deployment or API changes. Existing `demo-video/.gitignore` changes belong to other work and are preserved.
