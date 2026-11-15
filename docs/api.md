# API contract (v0, M1 + M2 scope)

Backend owns this file. Frontend builds against it. Change it in the same commit as the code.

Conventions: base `http://localhost:8080/api`; JSON; ids are UUID strings; timestamps RFC 3339; **money is a string of integer USDC minor units** (6 decimals, `"1500000"` = 1.5 USDC); `env` is `"arc_testnet"` or `"simulated"`; ratios are integer basis points (0–10000). Errors: HTTP 4xx/5xx with `{"error":{"code":"...","message":"...","details":[...]}}`. Single demo user, no auth (D-9).

Request bodies: every endpoint that takes a body requires `Content-Type: application/json` (optionally with `; charset=...`); anything else is **415** `unsupported_media_type` (browsers can send other types cross-site without a CORS preflight). A body over the size limit is **413** `payload_too_large`. Malformed JSON is 400; well-formed JSON of the wrong shape is 422.

Validation errors (422 `validation_failed`): `details` is `[{path, code, message}]`. `path` is a **dotted** field path into the request body: `name`, `config.horizon_secs`, `policy.max_per_interaction`, `graph` (whole-graph problems) or `graph.nodes.<node id>`; it is `""` when the body as a whole has the wrong shape.

## Types

```
AgentConfig  { goals, strategy, categories: string[], horizon_secs, risk_tolerance: "low"|"medium"|"high", schedule_secs }
AgentPolicy  { max_per_interaction: money, max_aggregate_exposure: money, approval_threshold: money,
               allowed_interaction_types: ("challenge"|"collaboration")[], min_conviction_bps, stop_loss: money|null }
Graph        { schema_version: 1, nodes: [{id, type, params: object}], edges: [{from, from_port, to, to_port}] }
Agent        { id, name, description, status: "active"|"paused", env, wallet_address: string|null, version: int,
               config: AgentConfig, policy: AgentPolicy, graph: Graph,
               balances: { available: money, committed: money },
               stats: { realized_pnl: money (signed), settled: int, wins: int }, created_at, updated_at }
AgentInput   { name, description, config, policy, graph }        // create and update body
Template     { id, name, description, config, policy, graph }
BlockDef     { type, kind: "trigger"|"reasoning"|"condition"|"action", label, description,
               inputs: [{port, data_type}], outputs: [{port, data_type}], params_schema: JSON Schema }
GraphError   { code, message, node_id: string|null }
LedgerEntry  { id, ts, env, account, delta: money (signed), reason, interaction_id: string|null, tx_hash: string|null }
ActivityItem { id, ts, agent_id: string|null, interaction_id: string|null, kind, summary, detail: object|null }
```

## Endpoints

| Method | Path | Body → Response | Notes |
|---|---|---|---|
| GET | `/health` | → `{status:"ok"}` | |
| GET | `/agents` | → `Agent[]` | |
| POST | `/agents` | `AgentInput` → `Agent` (201) | Validates policy and graph; 422 with details on failure |
| GET | `/agents/{id}` | → `Agent` | |
| PUT | `/agents/{id}` | `AgentInput` → `Agent` | Creates a new immutable version; history untouched |
| POST | `/agents/{id}/status` | `{status}` → `Agent` | pause / resume |
| GET | `/agents/{id}/versions` | → `[{version, created_at, config, policy, graph}]` | |
| GET | `/agents/{id}/ledger` | → `LedgerEntry[]` | newest first |
| POST | `/agents/{id}/allocate` | `{amount: money}` → `Agent` | Moves user unallocated → agent available. `simulated` env only in M1 |
| POST | `/agents/{id}/deallocate` | `{amount: money}` → `Agent` | Reverse; fails if amount > available |
| GET | `/templates` | → `Template[]` | |
| GET | `/graph/blocks` | → `BlockDef[]` | The block catalog the builder renders |
| POST | `/graph/validate` | `Graph` → `{valid: bool, errors: GraphError[]}` | Always 200 |
| GET | `/wallet` | → `{env, unallocated: money, allocated: money, committed: money, entries: LedgerEntry[]}` | User-level overview |
| POST | `/wallet/simulated-deposit` | `{amount: money}` → wallet | Credits simulated funds, labelled as such |
| GET | `/activity?agent_id=&interaction_id=&limit=` | → `ActivityItem[]` | newest first |

## M2: challenges

Simulated funds only. Nothing here signs or sends an onchain transaction. Resolution is **backend-administered** (the backend reads Gemini exchange market data and decides); it is not trustless, and the UI should say so.

**Every POST needs `Content-Type: application/json` and a JSON body; send `{}` when there is nothing to say** (approve, reject, tick). Without it the answer is 415.

### Types

```
MarketRef      { kind: "gemini_prediction_contract", event_ticker, instrument_symbol }
             | { kind: "gemini_spot", symbol }
ChallengeTerms { market: MarketRef, direction: "above"|"below",   // the PROPOSER's side
                 threshold: int,                                  // unit of `formula`
                 start_ts: int, end_ts: int,                      // unix seconds; start_ts = funding deadline
                 stake: money,                                    // per side
                 resolution_source: "gemini_prediction_markets"|"gemini_spot_ticker",
                 formula: "pm_yes_price_bps_v1"|"spot_last_price_micros_v1" }
Interaction    { id, kind: "challenge", env, state, proposer_id, proposer_name, counterparty_id, counterparty_name,
                 terms: ChallengeTerms, stake: money, market_title, market_source: "gemini"|"fake"|null,
                 claim: string,                 // "\"<market>\" will be at or above 0.55 (YES price)"
                 turn: agent id|null,           // whose move, while proposed / negotiating
                 rounds: int, max_rounds: int,  // offers made (proposal = 1) and the cap
                 pending_approvals: int,
                 outcome: "a"|"b"|"void"|null,  // a = proposer won, b = counterparty won, void = refunded
                 winner_id: string|null, observed: int|null, observed_display: string|null, observed_at,
                 status_reason: string|null,    // why it was cancelled / how it was resolved
                 resolution: "backend_administered",
                 start_at, end_at, created_at, updated_at }
InteractionDetail = Interaction + {
                 negotiation: [{ seq, agent_id, action: "propose"|"counter"|"accept"|"reject", terms, summary, ts }],
                 runs: [Run], approvals: [Approval], ledger: [LedgerEntry], activity: [ActivityItem] (oldest first) }
Run            { id, ts, agent_id, agent_version, trigger, status: "ok"|"no_action"|"error", error: string|null,
                 reasoner: "rule_based"|..., 
                 trace: [{ node_id, node_type, summary }],        // decision summaries, not model reasoning
                 intents: [{ intent: { type: "propose_challenge"|"accept_challenge"|"counter_offer"|"reject", stake? },
                             terms: ChallengeTerms|null, ask_owner: bool, summary }] }
Approval       { id, interaction_id, agent_id, agent_name, stake: money,
                 status: "pending"|"approved"|"rejected"|"expired"|"superseded",
                 reason, requested_at, decided_at: ts|null, expires_at, market_title, env }
TickReport     { now, proposed, denied, countered, accepted, rejected, approvals_requested, funded, resolving,
                 settled, voided, cancelled: int, errors: string[] }
Portfolio      { env, as_of,
                 totals: { unallocated, available, committed, allocated, at_risk: money,
                           realized_pnl: money (signed), unrealized_pnl: null },
                 pnl_over_time: [{ ts, interaction_id, agent_id, delta, agent_cumulative, portfolio_cumulative }],
                 allocation: [{ agent_id, name, available, committed }],
                 agents: [{ agent_id, name, realized_pnl, settled, wins, losses, win_rate_bps: int|null }],
                 outcomes: { settled, voided, cancelled, failed, open, active: int, by_state: { <state>: int } },
                 formulas: { <metric>: string } }
```

`state` is one of `proposed, negotiating, accepted, awaiting_approval, funded, active, resolving, settled, cancelled, disputed, failed`. A voided challenge is `cancelled` with `outcome: "void"`.

Units: `pm_yes_price_bps_v1` = the contract's YES price in basis points of 1.00 (`5500` = 0.55; `10000` / `0` once the contract resolved yes / no). `spot_last_price_micros_v1` = spot `last` in micro-USD. The "above" side wins when `observed >= threshold`, the "below" side when `observed < threshold`; no observation = void, both stakes refunded.

### Endpoints

| Method | Path | Body → Response | Notes |
|---|---|---|---|
| GET | `/interactions?agent_id=&state=&limit=` | → `Interaction[]` | newest first; `agent_id` matches either side; limit default 100, max 500 |
| GET | `/interactions/{id}` | → `InteractionDetail` | negotiation rounds, graph-run traces, approvals, ledger entries, activity |
| GET | `/approvals?status=` | → `Approval[]` | default `pending`; `all` or a specific status |
| POST | `/approvals/{id}/approve` | `{}` → `{approval, interaction}` | Records the approval, then **re-runs authorization** against current balances and limits. The interaction comes back `active`, still `awaiting_approval` (the other side is pending) or `cancelled` (authorization denied; reason in `status_reason`). 409 `approval_not_pending`, `approval_stale`, `approval_expired` |
| POST | `/approvals/{id}/reject` | `{}` → `{approval, interaction}` | Cancels the challenge; no funds move |
| GET | `/stats/portfolio` | → `Portfolio` | Every metric's formula is in `formulas` |
| POST | `/dev/tick` | `{}` or `{now: ts}` → `TickReport` | Runs one lifecycle step now (the interval scheduler calls the same code). `now` is a simulated clock, accepted only when the server runs on the fake market (422 otherwise). Not mounted when `DEV_ENDPOINTS=false` |
| POST | `/dev/fake-market` | `{move_bps?, yes_price_bps?, resolution_side?: "yes"|"no"|"open", prediction_markets?: bool, fail?: bool}` → `{status}` | Fake market only (404 otherwise) |

### Metrics

| Metric | Formula |
|---|---|
| available / committed | ledger balances of `agent:{id}:available` / `agent:{id}:committed` |
| allocated | available + committed |
| at_risk | committed |
| realized_pnl | net of the agent's `settle` ledger entries = sum over settled challenges of (payout − stake). Deposits, allocations, deallocations, stake commits and refunds are transfers and are excluded |
| unrealized_pnl | not computed (no fair mark for a binary challenge before it resolves) |
| win_rate_bps | wins × 10000 / settled; voided and cancelled challenges excluded |
| exposure (authorization) | sum of stakes of the agent's challenges in `funded`, `active`, `resolving` |

With one owner on both sides of a challenge, portfolio `realized_pnl` nets to zero; the per-agent numbers are the informative ones.

### Activity kinds added in M2

`challenge_proposed`, `proposal_denied`, `no_counterparty`, `challenge_countered`, `challenge_accepted`, `challenge_rejected`, `response_denied`, `negotiation_cap_reached`, `approval_requested`, `approval_granted`, `approval_rejected`, `commit_denied`, `stake_committed`, `challenge_expired`, `resolution_delayed`, `challenge_settled`, `challenge_voided`, `market_data_error`, `reasoning_failed`. Each has a full-sentence `summary`; `detail` carries machine-readable fields (`reasons` for denials, `terms`, `run_id`). Denials list `DenyReason`s: `agent_paused, stop_loss_triggered, interaction_type_not_allowed, illegal_interaction_state, zero_amount, amount_overflow, exceeds_max_per_interaction, exceeds_aggregate_exposure, insufficient_available_balance`.

Not yet specified: `/webhooks/quicknode`.
