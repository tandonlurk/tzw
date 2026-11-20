# TODO

You can edit this file freely: add, remove, reorder, or leave notes. Agents preserve your edits and only change status marks and their own lines.

Legend: `[ ]` not started · `[~]` in progress · `[!]` blocked · `[x]` done and verified · `[?]` awaiting your review

Deadline: event page says Devpost Fri Nov 20, 12 PM CST (AGENTS.md says Nov 19, see D-18). Demo flow working by end of Wed Nov 18. Details and dates: `tasks/project.md`.

## ⚠ Start here (2026-11-19 evening, read first)

Handoff: `reports/handoffs/2026-11-19-evening.md`.

- [x] **You:** merge PR #10 (owner confirmed 2026-11-19; it contains #8 and #9). Local checkout still needs syncing before implementation, coordinating with the demo-video agent.
- [ ] **You:** get an xAI key (console.x.ai → API Keys) into `backend/.env` as `XAI_API_KEY`, so Grok does real work (bounty). Then the agent runs one live Grok check
- [ ] **You:** answer the demo plan's questions (`demo-video/animation-plan.md`, end) and record voiceover + screen recordings; then an agent builds the animations (Remotion) and the assembly script
  - [x] Story outline (`demo-video/story-outline.md`) and the 10 full-frame animated scenes built (`demo-video/animations/README.md`, `npm run render`). Left: overlays 04/09/11 and the assembly script, after the recordings
  - 2026-11-19: owner has another agent working on the first demo-video steps; coordinate file ownership before further video edits.
- [ ] Optional: add vault addresses to the QuickNode filter (`docs/setup/quicknode-streams-filter.md`; addresses from `GET /api/streams`)
- [ ] Optional: delete the terminated QuickNode stream "dry-abandoned-plateau"; withdraw ~6 testnet USDC from the two Streams-test demo vaults
- [ ] ElevenLabs only if everything else is done

## Earlier start-here items (owner action)

- [x] Fund the Arc testnet addresses (done 2026-11-17: idx 0-2 have 20 USDC each; confirm MetaMask account too)
- [ ] Confirm MetaMask is set up: Arc Testnet added (`docs/setup/arc-testnet-funding.md` section 3), and the account you funded is the one you will connect. Needed for the browser wallet UI
- [ ] Then tell the agent "continue". Contracts are DEPLOYED (D-60). Next for the agent, in order:
  1. [x] Smoke run on Arc passed (D-61); `resolver()` reverted because the getter is `RESOLVER()`
  2. [x] Verified escrow, factory and AgentVault on Blockscout (D-61)
  3. [x] Chain wired into the lifecycle (D-63) and browser wallet UI built (D-62); live Arc run settled with drift 0 (`reports/handoffs/2026-11-17-arc-live-run.md`)
  4. [x] Committed the doc edits on `chain-vaults` (2f03581). Open PR: https://github.com/tandonlurk/tzw/pull/1
  5. [x] PRs #2–#5 merged into `main` (#1 closed). Still open for you: review the approach wheels and Firms pages, and confirm D-65 (80/20 reading, task fee as the "guarantee")
  6. [ ] **You:** try the wallet UI with MetaMask on Arc Testnet (create a vault for an agent, allocate, withdraw). Run instructions: `backend/README.md` "Run against Arc testnet"

## Needs you (review list)

- [x] Approve the plan in `docs/architecture.md` (owner said "start running the plan", 2026-11-14)
- [x] D-3 wallet custody: decided 2026-11-16 → per-agent `AgentVault` contracts owned by your browser wallet (D-57)
- [?] D-8 use Grok for agent reasoning now (behind a fallback), or keep agents rule-based until later?
- [x] Toolchain installed: Rust 1.100, Foundry 1.9, PostgreSQL 17 (dbs `aen_dev`, `aen_test`)
- [?] Provide when ready (in `.env`, never in chat): xAI API key, QuickNode key, Arc testnet faucet funds, ElevenLabs key
- [?] Confirm the event is the University Blockchain Conference 2026 hackathon (UT Austin) and the deadline is Nov 20, 12 PM CST (D-18)
- [?] D-14 base challenges on Gemini prediction-market events (the $4k Gemini bounty requires it)
- [?] Review defaults D-22..D-56 in `docs/decisions.md`. New on 2026-11-16: D-38/39 cooldown + P&L time, D-42..45 request guard / ledger hardening, D-46..48 collaborations (note: `/fund` takes `owner_approved: true` instead of the approvals queue), D-49..51 Streams webhook, D-52..56 custom code (code chooses whether/how much, not the claim)
- [?] D-11 custom code = JavaScript in a Wasmtime/QuickJS sandbox: spike says GO (`docs/research/sandbox-spike.md`); also review D-40 (build `runner.wasm` locally vs commit it)
- [?] D-17 main track "Agentic Payments & AI" + Arc award + all bounties
- [?] Ask at sponsor office hours (Nov 16-19): what "Grok Bot" means for the xAI bounty
- [?] Branch `afternoon-1118` (2026-11-18 evening, **uncommitted**: the agent was not allowed to commit): D-71 stale spot guard, xAI/QuickNode docs re-check. Review, then commit/merge. Handoff: `reports/handoffs/2026-11-18-afternoon.md`
- [x] **New UI (2026-11-19 overnight)**: merged as PR #7. Open http://localhost:3000, toggle Basic/Advanced (top-right icon or ⌘\\), try the Terminal and an agent's Logic and Code tabs. Review D-72, then merge. Handoff: `reports/handoffs/2026-11-19-ui-overnight.md`
- [x] Streams: stream "tzw-arc-testnet-escrow" live (Arc Testnet, Logs, escrow + factory filter) via a cloudflared tunnel; real deliveries verified 2026-11-19, confirmations in the UI (D-74, PR #10). Agent multiplier: D-73, PR #9
- [x] Pushed to GitHub `tandonlurk/tzw`; `main` = `recovery` as of 2026-11-16 (consider making `main` the default branch). Repo is PUBLIC: make it private if you do not want the docs visible

## M0 — Planning and research (Sat Nov 14)

- [x] Repository and environment inspection
- [x] Sponsor research: Arc, QuickNode Streams, Gemini exchange, xAI, ElevenLabs → `docs/research/sponsors.md`
- [x] Sandbox, builder and Rust crate research → `docs/research/runtime-and-sandbox.md`
- [x] Architecture, decisions, risk register drafted
- [x] Toolchain installed

## M1 — Foundation (Sun Nov 15)

- [x] `core`: money type, ledger rules, permission engine `authorize()`, state machine (81 tests) (backend, P0)
- [x] `core`: behavior-graph schema + validator, JSON Schema export (backend, P0)
- [x] Postgres schema + migrations; agent CRUD with versioned config/policy/graph; templates. 101 Rust tests pass (backend, P0)
- [x] `ChallengeEscrow.sol` + Foundry tests: 72 passing, local only, not deployed (contracts, P0)
- [x] Web shell, pages, creation form and basic builder: 15 unit tests, lint, build pass; browser-tested against the real backend 2026-11-15 (`reports/qa-m1.md`). Not browser-tested: add/connect blocks, incompatible-connection rejection (frontend, P0)

## M2 — Economic interaction (Mon Nov 16)

- [x] Graph executor + agent runtime (interval tick + `POST /api/dev/tick`), simulated env (backend, P0)
- [x] Gemini exchange market data incl. prediction-market contracts, spot fallback, deterministic fake; shapes in `docs/research/gemini-prediction-markets.md` (backend, P0)
- [~] Reasoner trait: rule-based done; Grok wired behind `XAI_API_KEY` but only mock-tested. Docs re-check 2026-11-18: model `grok-4.7` is listed; `/v1/chat/completions` is now labelled "Legacy" (still documented with `response_format`); schemas switched to documented `anyOf` + null (backend, P0/P1, needs D-8 + key)
- [x] Challenge proposal, bounded negotiation, approvals, resolve, settle/void in the simulated ledger; 211 backend tests (backend, P0)
- [x] Proposal cooldown (600 s per agent and market, D-33); `schemas/challenge_terms.schema.json`; demo seed `POST /api/dev/seed-demo` + seeded lifecycle test; 222 backend tests (backend, P1)
- [x] `AgentVault` + `VaultFactory` contracts: onchain caps, pause, operator revoke, owner withdraw; 100 Foundry tests (contracts, P0, D-57)
- [x] `aen-chain` crate (alloy): escrow + vault calls, receipt verification, owner helpers, anvil test kit; full lifecycle test on anvil (backend, P0)
- [x] Deploy escrow + factory to Arc testnet (D-60); Blockscout verified + live smoke run passed (D-61) (contracts, P0)
- [x] Wire chain into lifecycle (D-63; 341 backend tests; live Arc run settled, drift 0): create → both vaults stake → active → resolve/void → release; ledger from verified receipts; vault link/sync endpoints; test-owner dev endpoint for autonomous runs (backend, P0)
- [?] Browser wallet UI (built, merged 335d62b, D-62; not yet run with a real MetaMask signature or the real backend endpoints): connect, create vault per agent, allocate (amount or % of wallet), withdraw, live onchain balances + explorer links (frontend, P0)
- [x] Visual logic builder: add/connect/delete blocks, incompatible connections rejected, validate, save with positions; browser-tested (frontend, P0)
- [x] Agent detail + config/policy editing: version history with diffs, per-agent activity, "Saved as version N", 422 inline, config version on decision traces; browser-verified 2026-11-16 (frontend, P0)

## M3b — Approach block and firms (Tue Nov 17, owner request)

- [?] Approach block: stance wheel (competitive ↔ collaborative, cautious ↔ ambitious, firm role) + market wheel (any → category → subcategory, structure, keywords, symbols, ranges) with live preview; backend gates discovery and incoming challenges; ambition sizes stakes when an approach node exists (D-64). Design: `docs/design/approach-and-firms.md`
- [?] Firms: lead keeps 80%, scouts/analysts share 20% by role weight, optional task fee, losses on the lead; simulated only (D-64, D-65). E2E over HTTP + browser-checked against the real backend (`reports/handoffs/2026-11-18-approach-firms.md`)
- [x] Fake market catalog: BTC + 6 fixed events (ETH, WTI and natural gas under Commodities/Energy, gold, Fed decision, sports final), 14 contracts; `/markets/categories` and `/markets/preview` demo offline; BTC demo numbers unchanged (D-66) (backend, P1)
- [x] Live re-check 2026-11-18 evening: Gemini PM API back up; discovery picked `GEMI-BTC05M2611190140-UP` at book 0.50/0.51 → 5050 (D-67 mid verified live, contract had no last trade). Stale spot `last`: refuse to resolve when the latest trade is > 10 min old (wait, then void) (D-71). Live test: `cargo test -p aen-server live_gemini -- --ignored --nocapture` (backend, P1)
- [x] Testnet run gaps: vault release txs + `vault_released` activity; `/stats/portfolio?env=arc_testnet` + dashboard panel; discovery at bid/ask mid (D-67). Live Arc run settled, drift 0; mid not verified live (Gemini PM API 500 all night), stale spot `last` finding (`reports/handoffs/2026-11-18-testnet-gaps.md`) (backend, P1)
- [x] Firm demo seed: `POST /api/dev/seed-firm-demo` + README "Firm demo"; lead +3.80, scout 0.70, analyst 0.50 over HTTP (D-68) (backend, P2)
- [x] Starter templates with an approach node: bull 15 %/100 %, skeptic 25 %/50 %, cautious 30 %/20 %; demo and firm-demo numbers unchanged on a fresh-db rerun, Cautious stakes 0.72 (D-69) (backend, P2)

## M3 — Observability (Tue Nov 17)

- [x] QuickNode Streams webhook ingestion, local only: HMAC + skew check, idempotent on (tx hash, log index), reorg-aware, fixture tests (D-49..D-51). Header names, signed message and payload now match QuickNode's docs (2026-11-18); gzip gets a clear 415. Still not confirmed by a real delivery; not exposed (backend, P1)
- [x] Full lifecycle end to end on testnet with real Gemini data: autonomous scheduler, Gemini settled `no`, drift 0 (`reports/handoffs/2026-11-17-testnet-gemini.md`) (lead, P0)
- [x] Wallet overview, Activity view, approvals queue, interactions list/detail, Dashboard charts from `/stats/portfolio` only (per-agent P&L lines by default); 25 web tests. Browser-verified 2026-11-15 incl. reject, approve-then-cancelled notice, agent-detail challenges, template prefill (frontend, P0)
- [x] Backend gaps from `reports/qa-m2-web.md`: summaries now readable ("5.00 USDC (simulated)"); approvals arise on the counterparty side by design; dev clock jumps past a funding deadline are refused with 409 (D-34) (backend, P1)
- [x] Collaboration: agreement states, simulated payout split through ledger + `authorize` (D-46..D-48), list/detail/propose views, buttons from `actions`; browser-verified propose → accept → fund → complete and cancel 2026-11-16 (backend/frontend, P2)

## M4 — QA and security (Tue–Wed, continuous)

- [x] Permission-engine and ledger failure-case tests; contract fuzz tests: +35 Rust, +8 Foundry adversarial tests, findings in `reports/security-m1.md` (qa, P0)
- [x] Threat model → `docs/security/threat-model.md` (webhook, sandbox, custody are design-level only: features not built) (qa, P0)
- [x] Fixed SEC-01 (415 unless JSON), SEC-03 (double settle rejected), SEC-05 (413); repro tests now pass (backend, P0)
- [x] SEC-04 escrow deadline caps (7 d fund, 30 d resolve) and SEC-07 resolver cannot be a party: 84 Foundry tests, local only, D-32 (contracts, P1)
- [x] SEC-02 Host + Origin guard always on, bearer token when `AEN_API_TOKEN` set; SEC-08 TRUNCATE guard + balanced-set trigger; SEC-09 scoped idempotency keys; 266 backend tests (backend, P1). Web app: opt-in same-origin proxy adds the token server-side (D-70, 2026-11-18)
- [x] Overnight E2E regression 2026-11-18: README demo + co-captain + collaboration over HTTP on a fresh db, all numbers match; 14 web routes rendered in headless Chrome; fixed escrow tx hashes on the challenge page and the fake-market "Resolved by" label; 114 web tests (`reports/qa-overnight.md`) (qa, P1)

## M5 — Custom-code agents (Tue eve – Wed Nov 18) — committed, not optional

- [x] Sandbox host crate `aen-sandbox`: fuel, 500 ms epoch deadline, 32 MiB, no fs/network/env, strict intent output; spike GO (`docs/research/sandbox-spike.md`) (backend, P1)
- [x] `custom.code` block: JSON context in, `Intent`s out, validated then `authorize()`; test-run endpoint, 422 on save; 332 backend tests (D-52..D-56) (backend, P1)
- [x] Code editor (textarea, 64 KiB counter, starter), versioned save with inline 422, dry-run test-run with verdicts; browser-verified incl. denial and infinite-loop failure 2026-11-16; 45 web tests (frontend, P1)
- [x] Sandbox escape and resource-abuse tests: 27 in `aen-sandbox` (infinite loop, memory bomb, recursion, capability absence, bad output, concurrency); 293 backend tests (qa, P1)

## M6 — Co-captain, voice, demo (Wed Nov 18)

- [x] Co-captain text suggestions: backend (6 rules, list/generate/accept/dismiss, D-36/D-37) and "Co-captain" page (accept, dismiss, loosens-control warning); 254 backend + 30 web tests. Grok variant mock-tested only; suggestions with a proposed change were browser-tested from inserted rows, not rule-generated ones (backend/frontend, P2)
- [ ] ElevenLabs voice for co-captain (P3, last)
- [x] Demo script, seeded demo agents, README run instructions, AGENTS.md 9.4 filled in: simulated run hand-verified; testnet video script (setup, 15-shot list, fallbacks) rehearsed live on Arc 2026-11-18, bull +1.25 / skeptic -1.25, drift 0 (`docs/demo.md` "Testnet video script") (lead, P0)
- [~] Submission write-ups drafted in `docs/submission/` (one per award, honest status; refreshed 2026-11-18 overnight with all three Arc testnet runs, firms, and the unverified bid/ask mid); finalize after keys and record the video (lead, P0)

## Vision (owner, 2026-11-16) — after the hackathon unless time allows

- [ ] Optional TZW-held vault: users may choose to let TZW custody funds in exchange for a benefit (e.g. gas sponsorship, instant settlement, yield). Opt-in, never the default
- [ ] Pluggable custody providers: an agent's vault can be ours, a browser wallet, or another provider the user trusts more; same escrow interface
- [ ] Spot resolution on a windowed average: average of Gemini 1-minute candle closes over the last 5 minutes before the end time, void only with no candles (new formula id; replaces D-71's void-on-stale as the main path). Owner kept D-71 for the hackathon (2026-11-19)
- [ ] Minimal platform control and privacy: TZW should have little say over how agents interact. Open items: user-run operators (vault already lets the owner replace the operator), decentralised resolution instead of our resolver key (today resolution is backend-administered, D-6), private negotiation

## Notes from owner

- 2026-11-17 — storyboard drafted 2026-11-18 (`docs/design/explainers-storyboard.md`, no code); otherwise think about only: after the UI redesign (owner, Wed night), the app still needs animated explainers. Most of the product happens in the background (discovery, negotiation, escrow lock, resolution, firm payouts); the user mostly sees agent performance summaries and a transaction/position log, so the hard parts (escrow above all) need visual explaining. Owner will look for Claude visual tools/skills on GitHub.
