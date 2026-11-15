# Backend / Agent Runtime

- **Objective:** Rust core, API, runtime, integrations.
- **Scope:** backend/, schemas/. See docs/architecture.md §2-6, §8-9.
- **Dependencies:** Rust + Postgres installed; crate versions in docs/research/runtime-and-sandbox.md.
- **Assigned agent:** Backend / Agent Runtime specialist (branch `backend`)
- **Status:** M1 done (2026-11-14), awaiting lead review. M2 not started.

## Files changed
- `backend/` (new): workspace, `crates/core`, `crates/server`, `.env.example`, `README.md`.
- `schemas/graph.schema.json`, `schemas/blocks.json` (generated from `core`).
- `docs/api.md`: error shape, `ui` on nodes, `Idempotency-Key`, enums and rules spelled out.

## Work completed
- `core` (pure): `Money` / `SignedMoney`; double-entry posting sets (deposit, allocate, deallocate, commit stake, settle, refund); `AgentPolicy` validation; `Intent` + `authorize()` collecting every deny reason; interaction state machine; block catalog (15 blocks) + graph validator.
- `server`: migrations (agents, append-only agent_versions and ledger_entries, activity, 3 seeded templates), every endpoint in `docs/api.md`, env config, CORS, tracing, migrations on startup.
- Ledger writes: one transaction, advisory locks on the idempotency key and each touched account, then the balance check. Balances and stats are computed from the ledger on read.

## Tests run and results
- `cd backend && cargo test --workspace`: 101 passed, 0 failed (core 81 unit + 2 schema drift; server 1 unit + 17 integration against `aen_test`).
- `cargo fmt` clean, `cargo clippy --all-targets` no warnings.
- Manual E2E on `aen_dev` (port 8099): health, templates, create agent from a template, deposit, allocate, over-allocate (422), activity. Rows created were removed afterwards.

## Decisions made (defaults, owner may overturn; not yet in docs/decisions.md, which is the lead's file)
- **Stake accounting:** a stake moves `available -> committed` and stays in the agent's `committed` account until the interaction ends. At settlement both stakes pass through `escrow:{interaction}` to the winner's `available` in one posting set, so escrow nets to zero. Isolated in `core/src/ledger.rs` (`commit_stake`, `settle`, `refund`).
- **Approval boundary:** stake `<= approval_threshold` is automatic, strictly above needs approval. `authorize()` has no "approved" input: after owner approval call it again; `Deny` still blocks.
- **Paused / stop-loss:** a paused agent is denied everything. After the stop-loss (`realized_pnl <= -stop_loss`) only `Reject` is allowed. `stop_loss: "0"` is invalid (use null).
- **Legal states per intent:** propose = no interaction yet; accept / counter / reject = Proposed or Negotiating; commit funds = Accepted or AwaitingApproval.
- **State table:** `Funded -> Cancelled` and `Resolving -> Cancelled` are the timeout-refund paths; `Active` can only go to Resolving or Failed; `Disputed` is terminal.
- **Graph:** port data types `event | assessment | proposal | any`; "every path ends in an action" means a node with no outgoing edge must be an action (an unwired condition branch just stops).
- **Idempotency:** optional `Idempotency-Key` header on the three money-moving POSTs; same key + different request = 409.
- **Server binds 127.0.0.1 only** because there is no auth (D-9).

## Assumptions
- Single user, `simulated` env only in M1; `wallet_address` is always null until custody (D-3) is approved.
- `exposure` passed to `authorize()` will be the stake total of open interactions; the runtime that computes it is M2.

## Blockers
- None for M1. Chain work stays blocked on D-3 (custody), per AGENTS.md 7.6.

## Unresolved questions
- Should `committed` or `escrow:{interaction}` hold funds once an onchain deposit is confirmed? Current model keeps them in `committed` (see above).
- sqlx 0.9 could not determine the OS user inside the agent sandbox (it tried role `anonymous`), so URLs name the user explicitly (`postgres://$USER@localhost/...`). Unverified whether a bare `postgres://localhost/aen_dev` works in a normal terminal.
- Settlement stats (`realized_pnl`, `settled`, `wins`) are covered by a `core` test and one SQL query, but no endpoint produces settlements yet, so that SQL is untested end to end.

## Recommended next steps
- M2: interactions table + state transitions using `core::interaction`, `Reasoner` trait with the deterministic fallback, graph executor emitting `Intent`s through `authorize()`, approvals endpoints, Gemini market data.
- Add a settlement integration test as soon as interactions exist.
- Lead: copy the decisions above into `docs/decisions.md`.
