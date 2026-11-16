# TODO

You can edit this file freely: add, remove, reorder, or leave notes. Agents preserve your edits and only change status marks and their own lines.

Legend: `[ ]` not started · `[~]` in progress · `[!]` blocked · `[x]` done and verified · `[?]` awaiting your review

Deadline: event page says Devpost Fri Nov 20, 12 PM CST (AGENTS.md says Nov 19, see D-18). Demo flow working by end of Wed Nov 18. Details and dates: `tasks/project.md`.

## Needs you (review list)

- [x] Approve the plan in `docs/architecture.md` (owner said "start running the plan", 2026-11-14)
- [?] D-3 wallet custody: per-agent testnet keys from a backend seed (recommended) vs single operator wallet vs Circle wallets
- [?] D-8 use Grok for agent reasoning now (behind a fallback), or keep agents rule-based until later?
- [x] Toolchain installed: Rust 1.100, Foundry 1.9, PostgreSQL 17 (dbs `aen_dev`, `aen_test`)
- [?] Provide when ready (in `.env`, never in chat): xAI API key, QuickNode key, Arc testnet faucet funds, ElevenLabs key
- [?] Confirm the event is the University Blockchain Conference 2026 hackathon (UT Austin) and the deadline is Nov 20, 12 PM CST (D-18)
- [?] D-14 base challenges on Gemini prediction-market events (the $4k Gemini bounty requires it)
- [?] Review financial defaults D-22..D-37 in `docs/decisions.md` (resolution rule, void/refund, funding window, negotiation cap, exposure)
- [?] D-11 custom code = JavaScript in a Wasmtime/QuickJS sandbox: spike says GO (`docs/research/sandbox-spike.md`); also review D-40 (build `runner.wasm` locally vs commit it)
- [?] D-17 main track "Agentic Payments & AI" + Arc award + all bounties
- [?] Ask at sponsor office hours (Nov 16-19): what "Grok Bot" means for the xAI bounty
- [x] Pushed to GitHub `tandonlurk/tzw`; `recovery` is the default branch. Repo is PUBLIC: make it private if you do not want the docs visible

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
- [~] Reasoner trait: rule-based done; Grok wired behind `XAI_API_KEY` but only mock-tested, endpoint/model unverified (backend, P0/P1, needs D-8 + key)
- [x] Challenge proposal, bounded negotiation, approvals, resolve, settle/void in the simulated ledger; 211 backend tests (backend, P0)
- [x] Proposal cooldown (600 s per agent and market, D-33); `schemas/challenge_terms.schema.json`; demo seed `POST /api/dev/seed-demo` + seeded lifecycle test; 222 backend tests (backend, P1)
- [ ] Deploy escrow to Arc testnet; signer + chain executor (contracts/backend, P0, needs D-3 + faucet funds)
- [x] Visual logic builder: add/connect/delete blocks, incompatible connections rejected, validate, save with positions; browser-tested (frontend, P0)
- [x] Agent detail + config/policy editing: version history with diffs, per-agent activity, "Saved as version N", 422 inline, config version on decision traces; browser-verified 2026-11-16 (frontend, P0)

## M3 — Observability (Tue Nov 17)

- [ ] QuickNode Streams webhook ingestion, idempotent (backend, P1)
- [ ] Full lifecycle end to end on testnet: discover → negotiate → authorize → fund → resolve → settle (lead, P0)
- [x] Wallet overview, Activity view, approvals queue, interactions list/detail, Dashboard charts from `/stats/portfolio` only (per-agent P&L lines by default); 25 web tests. Browser-verified 2026-11-15 incl. reject, approve-then-cancelled notice, agent-detail challenges, template prefill (frontend, P0)
- [x] Backend gaps from `reports/qa-m2-web.md`: summaries now readable ("5.00 USDC (simulated)"); approvals arise on the counterparty side by design; dev clock jumps past a funding deadline are refused with 409 (D-34) (backend, P1)
- [ ] Collaboration: simple agreement + simulated payout split (backend/frontend, P2, first to cut)

## M4 — QA and security (Tue–Wed, continuous)

- [x] Permission-engine and ledger failure-case tests; contract fuzz tests: +35 Rust, +8 Foundry adversarial tests, findings in `reports/security-m1.md` (qa, P0)
- [x] Threat model → `docs/security/threat-model.md` (webhook, sandbox, custody are design-level only: features not built) (qa, P0)
- [x] Fixed SEC-01 (415 unless JSON), SEC-03 (double settle rejected), SEC-05 (413); repro tests now pass (backend, P0)
- [x] SEC-04 escrow deadline caps (7 d fund, 30 d resolve) and SEC-07 resolver cannot be a party: 84 Foundry tests, local only, D-32 (contracts, P1)
- [x] SEC-02 Host + Origin guard always on, bearer token when `AEN_API_TOKEN` set; SEC-08 TRUNCATE guard + balanced-set trigger; SEC-09 scoped idempotency keys; 266 backend tests (backend, P1). Web app sends no token yet

## M5 — Custom-code agents (Tue eve – Wed Nov 18) — committed, not optional

- [x] Sandbox host crate `aen-sandbox`: fuel, 500 ms epoch deadline, 32 MiB, no fs/network/env, strict intent output; spike GO (`docs/research/sandbox-spike.md`) (backend, P1)
- [ ] `custom.code` block: JSON context in, `Intent`s out, validated then `authorize()` (backend, P1)
- [ ] Code editor in agent creation, versioned save, test-run (frontend, P1)
- [x] Sandbox escape and resource-abuse tests: 27 in `aen-sandbox` (infinite loop, memory bomb, recursion, capability absence, bad output, concurrency); 293 backend tests (qa, P1)

## M6 — Co-captain, voice, demo (Wed Nov 18)

- [x] Co-captain text suggestions: backend (6 rules, list/generate/accept/dismiss, D-36/D-37) and "Co-captain" page (accept, dismiss, loosens-control warning); 254 backend + 30 web tests. Grok variant mock-tested only; suggestions with a proposed change were browser-tested from inserted rows, not rule-generated ones (backend/frontend, P2)
- [ ] ElevenLabs voice for co-captain (P3, last)
- [~] Demo script, seeded demo agents, README run instructions, AGENTS.md 9.4 filled in: simulated run done and hand-verified (`docs/demo.md`); testnet version waits on D-3 (lead, P0)
- [ ] Submission write-up per sponsor bounty (lead, P0)

## Notes from owner

(write here)
