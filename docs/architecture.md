# Architecture

Status: **proposed, awaiting owner review** (2026-11-14). Decisions and rationale live in `decisions.md`; verified external facts live in `research/`.

## 1. Shape

```
web/ (Next.js, TS)  ──HTTP/JSON──▶  backend/ (Rust)  ──SQLx──▶  PostgreSQL
  presentation only                   │  │
                                      │  └─Alloy──▶ Arc testnet ── ChallengeEscrow.sol (contracts/)
                                      │                  │
                                      │◀── webhook ── QuickNode Streams (contract events)
                                      ├──▶ Gemini exchange public market data (discovery + resolution price)
                                      └──▶ LLM reasoner (xAI Grok behind a trait; deterministic fallback)
```

Repository layout:

| Path | Contents | Owner |
|---|---|---|
| `backend/crates/core` | Pure domain logic, no IO: money type, ledger rules, permission engine, behavior-graph schema + validator, interaction state machine | Backend |
| `backend/crates/server` | Axum API, SQLx repos + migrations, agent runtime, graph executor, integrations (reasoner, market data, chain, streams webhook) | Backend |
| `backend/crates/sandbox` | Custom-code host (later milestone, M5) | Backend |
| `contracts/` | Foundry project: `ChallengeEscrow.sol`, tests, deploy script | Contracts |
| `web/` | Next.js app | Frontend |
| `schemas/` | JSON Schema for the behavior graph and API types, **generated from Rust**; the frontend consumes it | Backend (generated) |
| `docs/`, `tasks/`, `reports/`, `TODO.md` | Persistent project memory | Lead |

## 2. The one rule

Reasoning proposes; the permission engine decides; the executor acts.

```
trigger ─▶ behavior graph (blocks / LLM / custom code) ─▶ Intent ─▶ authorize() ─▶ Allow ─────────▶ executor (ledger + chain)
                                                                              ├─▶ RequireApproval ─▶ owner approves in UI ─▶ re-authorize ─▶ executor
                                                                              └─▶ Deny(reasons) ───▶ recorded in activity
```

- An `Intent` is a typed value (`ProposeChallenge`, `AcceptChallenge`, `CounterOffer`, `Reject`, `CommitFunds`, `ProposeCollaboration`, ...). Nothing else can cause a ledger or chain write.
- `authorize(intent, policy, ledger_snapshot, interaction_state) -> Decision` is a pure function in `core`. It checks: per-interaction max, aggregate exposure, available balance, allowed interaction type, approval threshold, valid counterparty/contract, current interaction state (AGENTS.md 8.2).
- Visual blocks, LLM output, custom code and (later) the co-captain all terminate in `Intent`s. None of them can reach the executor directly. This is what makes the builder and custom code safe to add.
- Approval is re-authorized at execution time, because balances may have changed since the request.

## 3. Agent definition

An agent = identity + config + policy + behavior graph + wallet context + history.

- **Config** (not enforceable): name, description, goals, strategy/personality text, preferred categories, horizon, schedule.
- **Policy** (enforced in Rust, mirrored onchain where possible): `max_per_interaction`, `max_aggregate_exposure`, `allowed_interaction_types`, `approval_threshold` (auto below, owner approval above), `stop_conditions` (e.g. halt after drawdown X), `min_conviction`.
- **Behavior graph**: versioned JSON, see section 4.
- Config, policy and graph are stored as immutable **versions**; interactions reference the version that produced them, so editing never rewrites history.
- Templates (e.g. "Momentum bull", "Skeptic", "Cautious") are seed rows that prefill config + policy + graph.

## 4. Behavior graph (visual builder output)

```json
{
  "schema_version": 1,
  "nodes": [{ "id": "n1", "type": "trigger.schedule", "params": { "every_secs": 60 } }],
  "edges": [{ "from": "n1", "from_port": "out", "to": "n2", "to_port": "in" }]
}
```

Initial block set (kept deliberately small):

| Kind | Blocks |
|---|---|
| Trigger | `trigger.schedule`, `trigger.market_move`, `trigger.challenge_received` |
| Reasoning | `reason.evaluate_opportunity`, `reason.evaluate_proposal`, `reason.generate_proposal` |
| Condition | `cond.conviction_gte`, `cond.exposure_within`, `cond.matches_strategy`, `cond.stake_lte` |
| Action | `action.propose_challenge`, `action.accept`, `action.counter`, `action.reject`, `action.request_approval` |
| Later | `custom.code` (M5) |

- The schema is defined in Rust (`core`), exported as JSON Schema to `schemas/`. The frontend renders blocks from that schema; it does not define them.
- Validation (Rust, on save and before every run): known node types, valid params, exactly one trigger per flow, acyclic, port types compatible, every path ends in an action. `POST /agents/{id}/graph/validate` exposes it to the builder.
- Conditions in the graph are **owner preferences**, not security. `cond.exposure_within` set looser than policy changes nothing: `authorize()` still applies the policy.
- Execution: event arrives, executor walks the graph from the matching trigger, reasoning nodes call the `Reasoner` trait, action nodes emit `Intent`s. Each run stores a concise trace (node id, inputs summary, output) for the activity view.

## 5. Custom code (M5, interface reserved now)

A `custom.code` node receives the same JSON context a reasoning node gets and returns a JSON list of `Intent`s. It runs in an isolated sandbox (WebAssembly, details in `research/runtime-and-sandbox.md`) with CPU, memory and time limits, no network, no filesystem, no keys. Its output is parsed and validated like LLM output and then goes through `authorize()`. Because the graph executor already treats nodes as `context -> outputs`, adding it later does not change the runtime.

## 6. Money

- All amounts are integer minor units of USDC (6 decimals). No floats anywhere in accounting.
- Every balance, entry and transaction carries `env`: `arc_testnet` or `simulated`. The UI labels both.
- **Double-entry ledger** in Postgres. Accounts: `user:unallocated`, `agent:{id}:available`, `agent:{id}:committed`, `escrow:{challenge_id}`, `external` (faucet/deposits). Every movement is a balanced entry set with a reason and an idempotency key.

| Metric | Formula |
|---|---|
| Available | balance of `agent:available` |
| Committed | balance of `agent:committed` |
| Allocated to agent | available + committed |
| Gross exposure | sum of stakes in interactions in `Funded`/`Active`/`Resolving` |
| Realized P&L | sum over settled interactions of (payout − stake). Allocations and withdrawals are excluded |
| Unrealized P&L | not shown for binary challenges in MVP (no fair mark); shown as "at risk" = committed |
| Win rate | wins / settled (refunds and cancellations excluded) |

## 7. Wallets and chain (proposed, **needs owner approval**, see `decisions.md` D-3)

- Each agent gets its own EVM address on Arc testnet. The agent's wallet holds its USDC, approves and deposits its own stake into escrow.
- Keys live only in a `Signer` module in the backend, behind a trait. They are never sent to the frontend, an LLM, or the sandbox. The signer only signs transactions built by the executor from an authorized `Intent`.
- `ChallengeEscrow.sol` (sketch): `create(id, partyA, partyB, stake, fundDeadline, resolveDeadline)`, `deposit(id)` (each party, exact stake, via USDC `transferFrom`), `resolve(id, winner)` (resolver role only, after both funded), `cancel/refund(id)` (anyone, if not fully funded by `fundDeadline`, or not resolved by `resolveDeadline + grace`: each party gets its stake back). Events for every transition. No admin withdrawal path.
- Resolution is **backend-administered**: the backend reads the Gemini exchange price and calls `resolve`. This is stated in the UI; it is not trustless. Disputes are out of MVP scope; the timeout refund is the safety net.
- Onchain events come back through QuickNode Streams (webhook) and are applied idempotently (keyed by tx hash + log index). The ledger entry for a deposit is confirmed by the event, not by the send.

## 8. Challenge lifecycle

`Proposed → Negotiating → Accepted → (AwaitingApproval) → Funded → Active → Resolving → Settled`, with `Cancelled`, `Failed`, `Disputed` (unused in MVP) as terminal branches. Transitions are a table in `core`; illegal transitions are rejected. The backend is the source of truth.

A challenge's terms are explicit and machine-checkable: `asset` (Gemini symbol), `direction` and `threshold_bps`, `start_ts`, `end_ts`, `stake` per side, `resolution_source` (endpoint + field), `formula`. Negotiation is bounded (max N rounds, then auto-reject).

## 9. API (sketch)

`/agents` CRUD + versions, `/agents/{id}/graph` (get, put, validate), `/templates`, `/wallet` (user overview), `/agents/{id}/ledger`, `/interactions` (+ `/{id}` detail with trace), `/approvals` (list, approve, reject), `/activity`, `/stats/portfolio`, `/webhooks/quicknode` (signature-verified), `/dev/tick` (demo-only trigger). Single demo user for the hackathon; auth is a documented gap unless time allows.

## 10. Later integrations

- **Co-captain (Grok)**: a read-only service that reads agent history and writes rows to a `suggestions` table. Accepting a suggestion calls the normal config-edit endpoint. It has no path to the executor.
- **Voice (ElevenLabs)**: a thin layer over co-captain text in and out. No new backend capability.
