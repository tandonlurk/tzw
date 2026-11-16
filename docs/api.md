# API contract (v0, M1 + M2 scope)

Backend owns this file. Frontend builds against it. Change it in the same commit as the code.

Conventions: base `http://localhost:8080/api`; JSON; ids are UUID strings; timestamps RFC 3339; **money is a string of integer USDC minor units** (6 decimals, `"1500000"` = 1.5 USDC); `env` is `"arc_testnet"` or `"simulated"`; ratios are integer basis points (0–10000). Errors: HTTP 4xx/5xx with `{"error":{"code":"...","message":"...","details":[...]}}`. Single demo user (D-9).

Request guard (SEC-02), always on, checked before any handler:
- **Host**: only `localhost`, `127.0.0.1`, `[::1]` (any port) plus `ALLOWED_HOSTS` are served; anything else is **403** `host_not_allowed` (DNS rebinding). A request without a Host header (not a browser) passes.
- **Origin**: a request carrying an `Origin` other than `CORS_ORIGIN` (default `http://localhost:3000`) is **403** `origin_not_allowed`, whatever the method (CSRF). No `Origin` (curl, server-side code) passes.
- **Token**: when `AEN_API_TOKEN` is set, every request except `GET /health` and `OPTIONS` needs `Authorization: Bearer <token>`, else **401** `unauthorized` (`WWW-Authenticate: Bearer`). The server refuses to start on a non-loopback `BIND_ADDR` without a token. The default local setup (no token, loopback, web app on `localhost:3000`) is unchanged.

Idempotency (SEC-09): `POST /wallet/simulated-deposit`, `/agents/{id}/allocate` and `/deallocate` accept an optional `Idempotency-Key` header (1-128 ASCII chars). Keys are scoped per caller and per env: a client key can never collide with the server's own posting keys or another caller's. Replaying a key with the same request returns the original result without posting again; the same key with a different amount, account, reason or interaction is **409** `idempotency_key_reused`.

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
LedgerEntry  { id, ts, env, account, delta: money (signed), reason, interaction_id: string|null, collaboration_id: string|null, tx_hash: string|null }
ActivityItem { id, ts, agent_id: string|null, interaction_id: string|null, collaboration_id: string|null, kind, summary, detail: object|null }
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
| GET | `/activity?agent_id=&interaction_id=&collaboration_id=&limit=` | → `ActivityItem[]` | newest first |

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
                 settled, voided, cancelled, suggestions: int, errors: string[] }
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
| POST | `/dev/tick` | `{}` or `{now: ts, allow_expiry?: bool}` → `TickReport` | Runs one lifecycle step now (the interval scheduler calls the same code). `now` is a simulated clock, accepted only when the server runs on the fake market (422 otherwise). A `now` at or after the funding deadline (`start_at`) of a challenge that is not funded yet is refused with 409 `clock_jump_would_expire` unless `allow_expiry: true` (then it expires, as in real time). Not mounted when `DEV_ENDPOINTS=false` |
| POST | `/dev/fake-market` | `{move_bps?, yes_price_bps?, resolution_side?: "yes"|"no"|"open", prediction_markets?: bool, fail?: bool}` → `{status}` | Fake market only (404 otherwise) |
| POST | `/dev/seed-demo` | `{}` → `{env: "simulated", agents: [{id, name, role: "proposer"|"counterparty", created}], funds_per_agent: Money, note}` | Idempotent. Creates "Demo Bull (simulated)" and "Demo Skeptic (simulated)" with fixed ids (`…00d1`, `…00d2`) from the starter templates and allocates 100 simulated USDC to each, once. Existing demo agents are left untouched. Uses the same validation and ledger code as `POST /agents` and `/allocate`; the agents get no special treatment afterwards |

Runtime rules worth knowing when reading these endpoints:

- **Proposal cooldown.** After an agent proposes on a market it does not propose on that market again for `PROPOSAL_COOLDOWN_SECS` (default 600, `0` disables), whatever happened to the first proposal. A proposal attempt that `authorize` denied (`proposal_denied`) or that found no counterparty (`no_counterparty`) starts the same cooldown, but only under the agent's current config version: an owner edit lets it try again on the next tick. Enforced in the runtime before any reasoning (no run, no model call); other markets are unaffected. The cooldown is measured on the lifecycle clock (the simulated `now` of `/dev/tick`).
- **Summaries are for people.** Negotiation `summary` strings and run traces write amounts as `2.50 USDC (simulated)`, prediction-market prices as `0.55 (YES price)`, spot prices as `65000.00 USD` and convictions / moves as percentages. The structured fields next to them (`terms.stake`, `terms.threshold`, `conviction_bps`) keep exact integers.
- `schemas/challenge_terms.schema.json` is the JSON Schema of `terms` (shape only; cross-field rules are checked by the server).

### Metrics

| Metric | Formula |
|---|---|
| available / committed | ledger balances of `agent:{id}:available` / `agent:{id}:committed` |
| allocated | available + committed |
| at_risk | committed |
| realized_pnl | net of the agent's `settle` ledger entries = sum over settled challenges of (payout − stake). Deposits, allocations, deallocations, stake commits and refunds are transfers and are excluded |
| unrealized_pnl | not computed (no fair mark for a binary challenge before it resolves) |
| win_rate_bps | wins × 10000 / settled; voided and cancelled challenges excluded |
| pnl_over_time | one point per agent per settled challenge, ordered by settlement time. `ts` is the lifecycle clock at settlement (the simulated `now` of the `/dev/tick` that settled it, real time otherwise), not the ledger rows' wall-clock `ts`; `agent_cumulative` / `portfolio_cumulative` are running sums of `delta` |
| exposure (authorization) | sum of stakes of the agent's challenges in `funded`, `active`, `resolving` |

With one owner on both sides of a challenge, portfolio `realized_pnl` nets to zero; the per-agent numbers are the informative ones.

### Activity kinds added in M2

`challenge_proposed`, `proposal_denied`, `no_counterparty`, `challenge_countered`, `challenge_accepted`, `challenge_rejected`, `response_denied`, `negotiation_cap_reached`, `approval_requested`, `approval_granted`, `approval_rejected`, `commit_denied`, `stake_committed`, `challenge_expired`, `resolution_delayed`, `challenge_settled`, `challenge_voided`, `market_data_error`, `reasoning_failed`. Each has a full-sentence `summary`; `detail` carries machine-readable fields (`reasons` for denials, `terms`, `run_id`). Denials list `DenyReason`s: `agent_paused, stop_loss_triggered, interaction_type_not_allowed, illegal_interaction_state, zero_amount, amount_overflow, exceeds_max_per_interaction, exceeds_aggregate_exposure, insufficient_available_balance`.

## Co-captain suggestions

Advisory only (AGENTS.md 2.3). A suggestion is text plus, sometimes, a small typed patch of an agent's limits. Nothing here moves funds, approves, or reaches `authorize` / the executor. Accepting a suggestion with a `proposed_change` is the owner editing the agent: the patch is merged into the agent's **current** version and goes through the same validation and versioning as `PUT /agents/{id}`.

```
Suggestion     { id, agent_id: uuid|null, agent_name: string|null,
                 kind: "approval_expiring"|"idle_funds"|"lower_max_stake"|"repeated_denials"|
                       "raise_approval_threshold"|"agent_halted"|"general",
                 title, body: string,                  // plain sentences for the owner
                 proposed_change: ProposedChange|null, // null = text only
                 evidence: object,                     // the real numbers it was derived from (money as strings)
                 loosens_control: bool,                // true when accepting raises a limit, the approval threshold or the stop-loss; show a warning
                 source: "rules"|"grok",
                 status: "pending"|"accepted"|"dismissed"|"expired",
                 created_at, decided_at: ts|null }
ProposedChange { policy?: { max_per_interaction?, max_aggregate_exposure?, approval_threshold?, stop_loss?: money,
                            min_conviction_bps?: int },
                 config?: { risk_tolerance?: "low"|"medium"|"high" } }   // only fields to change; nothing else can be named
```

| Method | Path | Body → Response | Notes |
|---|---|---|---|
| GET | `/suggestions?status=&agent_id=` | → `Suggestion[]` | newest first, max 200. `status` must be one of the four (422 otherwise) |
| POST | `/suggestions/generate` | `{}` → `{ source: "rules"|"grok", created: Suggestion[] }` | Reads the records and stores new suggestions. Uses Grok when `XAI_API_KEY` is set (`source: "rules"` means no key or a fallback). The scheduler / `/dev/tick` runs the rule-based part on every tick, so calling this is optional |
| POST | `/suggestions/{id}/accept` | `{}` → `Suggestion` | Text only: marks it `accepted`. With a `proposed_change`: applies it as a new agent version, then marks it. 422 `validation_failed` (same details as an agent edit) or `invalid_proposed_change` when the change is not valid any more; the suggestion then stays `pending`. Already accepted → 200, nothing applied twice. Dismissed / expired → 409 `suggestion_not_pending` |
| POST | `/suggestions/{id}/dismiss` | `{}` → `Suggestion` | Already dismissed → 200. Accepted / expired → 409 `suggestion_not_pending` |

De-duplication: at most one `pending` suggestion per agent and kind, and the same subject (this approval, this pause, this settle count, this agent version ...) is never suggested twice, so a dismissed suggestion does not return until the data changes. A pending rule suggestion whose condition went away becomes `expired` (kept, never deleted). Decided suggestions are immutable.

Rules (all from stored records; formulas in `server/src/cocaptain.rs`):

| Kind | Fires when | Proposes |
|---|---|---|
| `approval_expiring` | a pending approval's funding deadline is within 120 s | text |
| `idle_funds` | active agent with ≥ 1 USDC available, no open challenge and nothing for 3600 s; or an active agent with no funds while ≥ 1 USDC is unallocated | text |
| `lower_max_stake` | last 3 settlements all lost, or realized loss ≥ 20% of net allocation | `max_per_interaction` halved (and `approval_threshold` lowered to it if above) |
| `repeated_denials` | ≥ 3 denials for the same reason since the current version | per-challenge limit raised to the largest blocked stake (capped at the aggregate limit), or aggregate limit raised by it (both `loosens_control`); text for a low balance |
| `raise_approval_threshold` | last 3 owner decisions under the current version were all approvals | `approval_threshold` = largest of those stakes, capped at the per-challenge limit (`loosens_control`) |
| `agent_halted` | agent is paused, or realized loss reached its stop-loss | text |

`general` comes only from Grok. Grok output is untrusted: each item is parsed into the closed schema above, must name an agent it was shown, is validated like a rule suggestion and dropped if anything is off; evidence, `loosens_control` and `source` are filled in by the server. Activity kinds: `suggestion_accepted`, `suggestion_dismissed` (`detail.suggestion_id`, `detail.version`); an applied change also writes the usual `agent_updated`.

Not yet specified: `/webhooks/quicknode`.

## Collaborations (M3, simulated funds)

A two-agent agreement: a proposer and a partner each contribute a fixed amount; on completion the pooled contributions are paid out by the agreed split. The owner drives it over REST on behalf of the agents. Every agent decision (propose, accept, reject) and every commitment of funds (fund) goes through `core::authorize` with a fresh balance snapshot (same per-agent locks as challenges). Payouts and refunds release what was committed, by the funded terms, like a challenge settlement or refund, and are not gated again. Money moves only as balanced ledger posting sets (`collab_commit`, `collab_payout`, `collab_refund`). Completion is **declared by the owner** (`resolution: "owner_declared"`), not trustless.

States: `proposed → accepted | rejected`, `accepted → funded`, `funded → completed`, and `proposed | accepted | funded → cancelled`. `rejected`, `completed`, `cancelled` are final. `actions` lists what the owner may do next, so the UI can render buttons straight from it.

```
Collaboration {
  id, env: "simulated", state: "proposed"|"accepted"|"rejected"|"funded"|"completed"|"cancelled",
  objective: string,
  proposer: Party, partner: Party,
  pool: money,                      // proposer.contribution + partner.contribution
  resolution: "owner_declared",
  status_reason: string|null,       // the cancel reason, if any
  actions: ("accept"|"reject"|"fund"|"complete"|"cancel")[],
  created_at, updated_at, funded_at: ts|null, closed_at: ts|null
}
Party { agent_id, name, contribution: money, share_bps: int, expected_payout: money,
        payout: money|null, net: money (signed)|null }   // payout and net = payout - contribution, once completed
```

Split: `proposer.expected_payout = floor(pool × proposer_share_bps / 10000)`, `partner.expected_payout = pool − that` (the rounding unit goes to the partner, so nothing is created or lost). Contributions must both be > 0; `proposer_share_bps` is 0–10000; objective 1–500 characters.

| Method | Path | Body → Response | Notes |
|---|---|---|---|
| GET | `/collaborations?agent_id=&state=` | → `Collaboration[]` | newest first, max 200; `agent_id` matches either party; unknown `state` → 400 |
| POST | `/collaborations` | `{proposer_id, partner_id, objective, proposer_share_bps, proposer_contribution: money, partner_contribution: money}` → **201** `Collaboration` | Authorizes the proposer (`ProposeCollaboration`). 422 `validation_failed` (paths `objective`, `proposer_share_bps`, `proposer_contribution`, `partner_contribution`, `partner_id` = same agent), 404 unknown agent, 422 `unsupported_env` |
| GET | `/collaborations/{id}` | → `{collaboration: Collaboration, ledger: LedgerEntry[], activity: ActivityItem[]}` | ledger and activity oldest first |
| POST | `/collaborations/{id}/accept` | `{}` → `Collaboration` | Partner, authorized like a proposal of its own contribution (`ProposeCollaboration`) |
| POST | `/collaborations/{id}/reject` | `{}` → `Collaboration` | Partner, `Reject` |
| POST | `/collaborations/{id}/fund` | `{owner_approved?: bool}` → `Collaboration` | Both parties re-authorized (`CommitFunds`, type `collaboration`) against current balances and limits, then both contributions move available → committed in one transaction. A contribution above an agent's `approval_threshold` needs `owner_approved: true`, else **409** `approval_required` (`details[].path` = `proposer`/`partner`). Funded contributions count toward `max_aggregate_exposure` |
| POST | `/collaborations/{id}/complete` | `{}` → `Collaboration` | Pays the split. Exactly once (row lock + unique ledger indexes); a later pause or policy change does not block it |
| POST | `/collaborations/{id}/cancel` | `{reason?: string (≤ 200)}` → `Collaboration` | Unfunded: no money moves. Funded: both contributions refunded |

Errors: a step in the wrong state is **409** `invalid_state`; a refusal by the permission engine is **422** `authorization_denied` with one detail per reason, `{path: "proposer"|"partner", code: <DenyReason>, message}` (codes as in challenges: `agent_paused`, `stop_loss_triggered`, `interaction_type_not_allowed`, `exceeds_max_per_interaction`, `exceeds_aggregate_exposure`, `insufficient_available_balance`, ...) and a `collaboration_denied` activity entry. Pausing an agent or changing its policy after funding never traps money.

Accounting: contributions sit in each agent's `committed` balance while funded; the payout set moves both into `escrow:{collaboration id}` and from there to each party's `available`. Collaboration results are **not** part of `realized_pnl` (challenge P&L) in the MVP; use `Party.net`.

Activity kinds (all carry `collaboration_id`; amounts in summaries say "(simulated)"): `collaboration_proposed` (proposer), `collaboration_accepted` / `collaboration_rejected` (partner), `collaboration_funded` (one per party, `detail.owner_approved`), `collaboration_completed` (one per party, `detail.contribution`, `payout`, `net`), `collaboration_cancelled` (one per party with `detail.refund` if it was funded, else one), `collaboration_denied` (`detail.step`, `role`, `reasons`).

Example: create, then complete.

```json
POST /api/collaborations
{ "proposer_id": "00000000-0000-4000-8000-0000000000d1",
  "partner_id": "00000000-0000-4000-8000-0000000000d2",
  "objective": "Research ETH funding rates and share the findings",
  "proposer_share_bps": 6000, "proposer_contribution": "3000000", "partner_contribution": "5000000" }

201
{ "id": "6f1c…", "env": "simulated", "state": "proposed",
  "objective": "Research ETH funding rates and share the findings",
  "proposer": { "agent_id": "…00d1", "name": "Demo Bull (simulated)", "contribution": "3000000",
                "share_bps": 6000, "expected_payout": "4800000", "payout": null, "net": null },
  "partner":  { "agent_id": "…00d2", "name": "Demo Skeptic (simulated)", "contribution": "5000000",
                "share_bps": 4000, "expected_payout": "3200000", "payout": null, "net": null },
  "pool": "8000000", "resolution": "owner_declared", "status_reason": null,
  "actions": ["accept", "reject", "cancel"],
  "created_at": "2026-11-16T09:00:00Z", "updated_at": "2026-11-16T09:00:00Z",
  "funded_at": null, "closed_at": null }

POST /api/collaborations/6f1c…/complete   {}     (after accept and fund)
200 { …, "state": "completed", "actions": [],
      "proposer": { …, "payout": "4800000", "net": "1800000" },
      "partner":  { …, "payout": "3200000", "net": "-1800000" } }
```

Activity entry:

```json
{ "id": "…", "ts": "2026-11-16T09:05:00Z", "agent_id": "…00d1", "interaction_id": null,
  "collaboration_id": "6f1c…", "kind": "collaboration_completed",
  "summary": "Collaboration \"Research ETH funding rates and share the findings\" completed: \"Demo Bull (simulated)\" received 4.80 USDC (simulated) for a 3.00 USDC (simulated) contribution (net +1.80 USDC (simulated)).",
  "detail": { "contribution": "3000000", "payout": "4800000", "net": "1800000", "env": "simulated", "resolution": "owner_declared" } }
```
