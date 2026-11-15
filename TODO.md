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
- [?] D-11 custom code = JavaScript in a Wasmtime/QuickJS sandbox
- [?] D-17 main track "Agentic Payments & AI" + Arc award + all bounties
- [?] Ask at sponsor office hours (Nov 16-19): what "Grok Bot" means for the xAI bounty
- [?] Push to GitHub (`origin` = tandonlurk/tzw): not done yet, ask when ready

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
- [~] Web shell, pages, creation form and basic builder: build/lint/15 unit tests passed before the data loss; NOT yet tested against the real backend (frontend, P0)

## M2 — Economic interaction (Mon Nov 16)

- [ ] Graph executor + agent runtime (event-driven + scheduled tick) (backend, P0)
- [ ] Gemini exchange market data: discovery signal and resolution price (backend, P0)
- [ ] Reasoner trait: rule-based fallback, then Grok (backend, P0/P1, needs D-8)
- [ ] Challenge proposal, bounded negotiation, approvals queue (backend, P0)
- [ ] Deploy escrow to Arc testnet; signer + chain executor (contracts/backend, P0, needs D-3 + faucet funds)
- [ ] Visual logic builder: canvas, block palette from schema, save/validate/reuse (frontend, P0)
- [ ] Agent detail + config/policy editing (frontend, P0)

## M3 — Observability (Tue Nov 17)

- [ ] QuickNode Streams webhook ingestion, idempotent (backend, P1)
- [ ] Full lifecycle end to end on testnet: discover → negotiate → authorize → fund → resolve → settle (lead, P0)
- [ ] Wallet overview, Activity view, Dashboard charts from real data (frontend, P0)
- [ ] Collaboration: simple agreement + simulated payout split (backend/frontend, P2, first to cut)

## M4 — QA and security (Tue–Wed, continuous)

- [ ] Permission-engine and ledger failure-case tests; contract fuzz tests (qa, P0)
- [ ] Threat model: keys, webhook auth, LLM output validation, agent/user isolation → `docs/security/` (qa, P0)

## M5 — Custom-code agents (Tue eve – Wed Nov 18) — committed, not optional

- [ ] Sandbox host crate with CPU/memory/time limits, no network/filesystem (backend, P1)
- [ ] `custom.code` block: JSON context in, `Intent`s out, validated then `authorize()` (backend, P1)
- [ ] Code editor in agent creation, versioned save, test-run (frontend, P1)
- [ ] Sandbox escape and resource-abuse tests (qa, P1)

## M6 — Co-captain, voice, demo (Wed Nov 18)

- [ ] Grok co-captain text suggestions, accept/dismiss (backend/frontend, P2)
- [ ] ElevenLabs voice for co-captain (P3, last)
- [ ] Demo script, seeded demo agents, README run instructions, AGENTS.md 9.4 filled in (lead, P0)
- [ ] Submission write-up per sponsor bounty (lead, P0)

## Notes from owner

(write here)
