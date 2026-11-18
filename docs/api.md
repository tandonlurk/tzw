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
Agent        { id, name, description, status: "active"|"paused", env, wallet_address: string|null,
               vault_address: string|null, vault_owner: string|null,   // Arc testnet vault (lowercase hex), see "Arc testnet"
               version: int,
               config: AgentConfig, policy: AgentPolicy, graph: Graph,
               balances: { available: money, committed: money },
               stats: { realized_pnl: money (signed), settled: int, wins: int, firm_income: money },
               created_at, updated_at }
               // realized_pnl: settled challenge P&L, net of what the agent paid firm members (task fees and
               //   profit shares). firm_income: what it received as a firm member; never part of realized_pnl.
AgentInput   { name, description, config, policy, graph }        // create and update body
Template     { id, name, description, config, policy, graph }
BlockDef     { type, kind: "trigger"|"reasoning"|"condition"|"action"|"approach", label, description,
               inputs: [{port, data_type}], outputs: [{port, data_type}], params_schema: JSON Schema }
GraphError   { code, message, node_id: string|null, param_path?: string }
             // param_path: only on invalid_params of an approach.profile node (dotted field inside params).
             // In a 422 the detail path is then `graph.nodes.<id>.params.<param_path>`; every other graph
             // error keeps `graph.nodes.<id>`. New code: `multiple_approaches`.
LedgerEntry  { id, ts, env, account, delta: money (signed), reason, interaction_id: string|null, collaboration_id: string|null,
               tx_hash: string|null }   // tx_hash: the onchain transaction behind the entry (arc_testnet), else null
ActivityItem { id, ts, agent_id: string|null, interaction_id: string|null, collaboration_id: string|null,
               firm_id: string|null, kind, summary, detail: object|null }
               // GET /activity also takes ?firm_id=
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
| GET | `/templates` | → `Template[]` | Each starter graph carries an `approach.profile` node (D-69): Momentum bull collaboration 15 % / ambition 100 %, Skeptic 25 % / 50 %, Cautious 30 % / 20 %; firm role none, any market |
| GET | `/graph/blocks` | → `BlockDef[]` | The block catalog the builder renders |
| POST | `/graph/validate` | `Graph` → `{valid: bool, errors: GraphError[]}` | Always 200 |
| GET | `/wallet` | → `{env, unallocated: money, allocated: money, committed: money, entries: LedgerEntry[]}` | User-level overview |
| POST | `/wallet/simulated-deposit` | `{amount: money}` → wallet | Credits simulated funds, labelled as such |
| GET | `/activity?agent_id=&interaction_id=&collaboration_id=&firm_id=&limit=` | → `ActivityItem[]` | newest first |

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
                 start_at, end_at, created_at, updated_at,
                 escrow: { create_tx, proposer_stake_tx, counterparty_stake_tx, settle_tx, proposer_release_tx, counterparty_release_tx: string|null }|null,  // arc_testnet only; a release tx stays null when that vault had nothing staked
                 firm_id: string|null }  // set on a firm lead's firm challenges (see "Approach and firms")
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
TickReport     { now, proposed, denied, countered, accepted, rejected, approvals_requested, funded, activated,
                 resolving, settled, voided, cancelled, suggestions, firm_joined, firm_contributions: int,
                 errors: string[] }
                 // funded: ledger reservations made during this tick (accepted -> funded). An owner approval via
                 //   POST /approvals/{id}/approve reserves outside a tick and is not counted, so `funded: 0` is
                 //   normal after an approval.
                 // activated: arc_testnet challenges whose escrow went live this tick (funded -> active, both
                 //   stakes deposited onchain). Simulated challenges go straight to active and are not counted.
                 // firm_joined / firm_contributions: see "Approach and firms".
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

Units: `pm_yes_price_bps_v1` = the contract's YES price in basis points of 1.00 (`5500` = 0.55; `10000` / `0` once the contract resolved yes / no). `spot_last_price_micros_v1` = spot `last` in micro-USD. Discovery sets a prediction-contract threshold from the YES bid/ask mid (rounded down) when both sides are quoted and not crossed, else the last trade (D-67); resolution reads as above. The "above" side wins when `observed >= threshold`, the "below" side when `observed < threshold`; no observation = void, both stakes refunded.

### Endpoints

| Method | Path | Body → Response | Notes |
|---|---|---|---|
| GET | `/interactions?agent_id=&state=&limit=` | → `Interaction[]` | newest first; `agent_id` matches either side; limit default 100, max 500 |
| GET | `/interactions/{id}` | → `InteractionDetail` | negotiation rounds, graph-run traces, approvals, ledger entries, activity |
| GET | `/approvals?status=` | → `Approval[]` | default `pending`; `all` or a specific status |
| POST | `/approvals/{id}/approve` | `{}` → `{approval, interaction}` | Records the approval, then **re-runs authorization** against current balances and limits. The interaction comes back `active` (`funded` for an `arc_testnet` challenge: the vaults stake on the next tick), still `awaiting_approval` (the other side is pending) or `cancelled` (authorization denied; reason in `status_reason`). 409 `approval_not_pending`, `approval_stale`, `approval_expired` |
| POST | `/approvals/{id}/reject` | `{}` → `{approval, interaction}` | Cancels the challenge; no funds move |
| GET | `/stats/portfolio?env=simulated\|arc_testnet` | → `Portfolio` | One env per response (default `simulated`, anything else 400; never summed across envs, D-67). Every metric's formula is in `formulas` |
| POST | `/dev/tick` | `{}` or `{now: ts, allow_expiry?: bool}` → `TickReport` | Runs one lifecycle step now (the interval scheduler calls the same code). `now` is a simulated clock, accepted only when the server runs on the fake market (422 otherwise). A `now` at or after the funding deadline (`start_at`) of a challenge that is not funded yet is refused with 409 `clock_jump_would_expire` unless `allow_expiry: true` (then it expires, as in real time). Not mounted when `DEV_ENDPOINTS=false` |
| POST | `/dev/fake-market` | `{move_bps?, yes_price_bps?, resolution_side?: "yes"|"no"|"open", prediction_markets?: bool, fail?: bool}` → `{status}` | Fake market only (404 otherwise). Prices and resolution apply to the BTC contract only; `move_bps` moves both fake spot prices (BTC, ETH); `prediction_markets: false` hides the whole fake catalog (D-66) |
| POST | `/dev/seed-demo` | `{}` → `{env: "simulated", agents: [{id, name, role: "proposer"|"counterparty", created}], funds_per_agent: Money, note}` | Idempotent. Creates "Demo Bull (simulated)" and "Demo Skeptic (simulated)" with fixed ids (`…00d1`, `…00d2`) from the starter templates and allocates 100 simulated USDC to each, once. Existing demo agents are left untouched. Uses the same validation and ledger code as `POST /agents` and `/allocate`; the agents get no special treatment afterwards |
| POST | `/dev/seed-firm-demo` | `{}` → `{env: "simulated", agents: [{id, name, role: "scout"\|"analyst"\|"counterparty"\|"lead", created}], firm_id, firm_created, funds_per_agent: Money, note}` | Idempotent. Creates Firm Scout, Firm Analyst, Firm Rival and Firm Lead "(simulated)" (ids `…00f1`..`…00f4`, 100 simulated USDC each, once) with an `approach.profile` node each, and the open firm `…00fa` (lead 80 %, task fee 0.10, one scout and one analyst slot, 60/40) with both members joined, through the same code as `POST /firms` and `/join`. Existing agents, firm and members are left untouched. Independent of `/dev/seed-demo`, but use a fresh database (agents of both seeds can trade with each other). Walkthrough: `backend/README.md` "Firm demo" |

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

## QuickNode Streams webhook (local ingestion only)

`POST /api/webhooks/quicknode` stores Arc testnet logs delivered by a QuickNode Stream (dataset `Logs`, webhook destination, **no compression**) and records them as activity. It is **mounted only when `QUICKNODE_STREAMS_SECRET` is set** (404 otherwise). Nothing acts on the events yet: there is no chain executor, and nothing signs or sends a transaction. No tunnel is set up by this repo; exposing the route is an owner decision.

**Authentication: the webhook's own HMAC, not the bearer token.**
- Signature: lowercase hex HMAC-SHA256 with the stream's security token as key, over `nonce + timestamp + raw body` (string concatenation, no separators); an optional `sha256=` prefix and upper-case hex are accepted. Missing signature or timestamp → **401** `missing_signature`; mismatch → **401** `invalid_signature`.
- Timestamp: unix seconds (or milliseconds, if > 10^12), within `QUICKNODE_MAX_SKEW_SECS` (default 300) of the server clock, else **401** `stale_timestamp`. Checked after the signature, so it is authenticated too.
- Header names (**UNVERIFIED**, configurable): `QUICKNODE_SIGNATURE_HEADER` (default `x-qn-signature`), `QUICKNODE_TIMESTAMP_HEADER` (`x-qn-timestamp`), `QUICKNODE_NONCE_HEADER` (`x-qn-nonce`; a missing nonce is signed as the empty string). The signed-message layout is also unverified: capture one real delivery and adjust before relying on it.
- Then: `Content-Type: application/json` (415 otherwise), JSON (400), shape (422), at most 5000 logs and the default 2 MB body limit (413).

**Request guard (SEC-02) and this route.** QuickNode sends no `Origin`, so the Origin check passes; a request that does carry a foreign `Origin` is still refused. Its `Host` is the public tunnel host, which must be listed in `ALLOWED_HOSTS` (otherwise 403 `host_not_allowed`). Any extra `ALLOWED_HOSTS` entry makes `AEN_API_TOKEN` **required at startup**, because a tunnel to the loopback port exposes every route: with the token set, every other route needs `Authorization: Bearer …`, while `POST /api/webhooks/quicknode` is exempt from the token and authenticated by its HMAC alone.

**Idempotency.** Each log is stored once, keyed on `(tx_hash, log_index)` (`chain_events` table, unique). A duplicate, a repeated batch, a reordered batch, or concurrent deliveries change nothing. A later delivery of a known log with `removed: true` (reorg) marks it removed once.

**Assumed payload (UNVERIFIED)**: QuickNode's documented envelope `{ data: [...], metadata: {...} }` with `data` holding logs in `eth_getLogs` format, either flat or one array per block; a bare array, and receipt-shaped `{logs: [...]}` elements, are accepted too. Per log the server reads: `transactionHash` (0x + 64 hex), `logIndex` and `blockNumber` (hex quantity or number), `address` (0x + 40 hex), `topics` (≤ 4 × 0x + 64 hex), `data` (hex), `blockHash` (optional), `removed` (optional bool). Hex is stored lower-case. A log that fails these checks is skipped and counted in `rejected`; the others are stored. `metadata.network` and `metadata.stream_id` are kept with each event. Fixture: `backend/crates/server/tests/fixtures/quicknode/logs_batch.json`.

```json
{ "data": [[ { "address": "0x5fbdb2315678afecb367f032d93f642f64180aa3",
               "blockHash": "0x1d59…8df2", "blockNumber": "0x1b4", "data": "0x…4c4b40",
               "logIndex": "0x0", "removed": false,
               "topics": ["0xddf252ad…b3ef", "0x…2266", "0x…79c8"],
               "transactionHash": "0x88df…944b", "transactionIndex": "0x0" } ]],
  "metadata": { "network": "arc-testnet", "dataset": "logs", "stream_id": "…",
                "batch_start_range": 436, "batch_end_range": 437 } }
```

Response **200** (QuickNode retries anything else): `{ received, inserted, duplicates, removed, rejected }`, e.g. `{"received":3,"inserted":3,"duplicates":0,"removed":0,"rejected":0}` and, for the same batch again, `{"received":3,"inserted":0,"duplicates":3,"removed":0,"rejected":0}`.

Activity kinds (`agent_id` null; `detail`: `chain_event_id`, `source: "quicknode_streams"`, `env: "arc_testnet"`, `network`, `tx_hash`, `log_index`, `block_number`, `address`, `topic0`): `chain_event` ("Arc testnet event seen via QuickNode Streams: log 0 of transaction 0x88df0164…944b from contract 0x5fbdb231…0aa3 in block 436. Recorded only; nothing acts on it yet."), `chain_event_removed`, and `chain_events` (one summary row when a delivery has more than 50 new events).

## Custom code (sandboxed JavaScript)

Graph block `custom.code` (kind `action`, input port `in` accepts any type, no outputs), params `{source: string}` with 1 character to 64 KiB (UTF-8 bytes) of JavaScript defining `function decide(ctx)`. It runs in a QuickJS guest inside Wasmtime with no network, files, keys or real clock, and limits of 50M fuel (~7 ms CPU), 500 ms wall clock, 32 MiB memory and 64 KiB output. `Date` is fixed to `ctx.now_ms` and `Math.random` is seeded per agent and tick, so a run replays exactly. A graph may use it after any trigger, e.g. `trigger.schedule → custom.code` or `trigger.challenge_received → custom.code`.

`decide` returns an array of at most 10 intents (money as digit strings, no leading zeros, at most 18 digits; unknown types or fields are rejected):

```
{type:"propose_challenge", stake}        // looking for opportunities
{type:"accept_challenge", stake}         // answering: stake must equal the stake on the table
{type:"counter_offer", stake}            // answering: same terms, different stake
{type:"reject"}                          // do nothing / decline
```

`commit_funds` and `propose_collaboration` parse but are refused (the runtime commits stakes itself once both sides agree). Every usable intent then goes through `core::authorize` exactly like a built-in block's: over-limit stakes are denied (`proposal_denied`), stakes above `approval_threshold` wait for the owner. A proposal's claim (side, threshold, funding deadline, end) is the deterministic rule-based template shown in `ctx.proposal`; the code chooses whether to propose and the stake.

`ctx` (no policy limits, keys, addresses or other agents' data):

```
{ trigger: "trigger.schedule"|"trigger.market_move"|"trigger.challenge_received", now_ms: int,
  agent: { id, name, version, config: AgentConfig },
  balances: { available: money, exposure: money },
  market: { market: MarketRef, title, category, value: int, move_bps: int, window_secs, spot_symbol, source, fallback_reason },
  proposal: { direction: "above"|"below", threshold: int, formula, start_ts, end_ts } | null,   // discovery only
  incoming: { terms: ChallengeTerms, my_side: "above"|"below" } | null }                      // answering only
```

Failures never move funds. A sandbox error (CPU, time, memory, stack, a throw, invalid output, an unsupported feature such as `import()`), an intent the flow cannot use, or a server without the sandbox ends the run with no intents: `agent_runs` gets `status: "error"`, `reasoner: "custom_code"`, and the activity gets a `custom_code_failed` entry (logged once while it persists; `detail.code = {node_id, fuel_used, logs, error, module_sha256}`). Successful runs record `reasoner: "custom_code"` with the fuel and `console` lines in the trace.

Save check: `POST /agents` and `PUT /agents/{id}` run every `custom.code` source once against a sample `ctx` (`market.source = "sample"`, plus a sample `incoming` challenge for a `trigger.challenge_received` flow). Any sandbox error is **422** `validation_failed` with `{path: "graph.nodes.<id>", code: "custom_code_failed", message}`. It is skipped when the server has no sandbox. It is a usability check, not a security boundary.

| Method | Path | Body → Response | Notes |
|---|---|---|---|
| POST | `/agents/{id}/code/test-run` | `{source?: string}` → `TestRun` | Runs the agent's saved `custom.code` (or `source`) against the current market and its real balances. **Writes nothing** (no ledger, run, activity, interaction or approval). 404 unknown agent; 422 `no_custom_code` (no block and no `source`) or `validation_failed` (`source` empty or over 64 KiB); 503 `sandbox_unavailable`, `market_data_unavailable` |

```
TestRun { ok: bool,
          error: { code: "out_of_fuel"|"timeout"|"memory_limit"|"stack_overflow"|"output_too_large"|"guest_error"
                         |"bad_output"|"unsupported_feature"|"code_too_large"|"input_too_large"|"trap"|..., message } | null,
          intents: [{ intent: Intent, decision: { result: "allow"|"require_approval"|"deny", reasons: DenyReason[] } }],
          logs: string[], fuel_used: int|null, ctx: object, module_sha256: string, simulated: true }
```

The verdict is `core::authorize` as the intent would be judged when used: a proposal as a new interaction, an answer in state `proposed`, `commit_funds` in state `accepted`.

## Arc testnet (onchain vaults and escrow)

Enabled only when the server has `AEN_TESTNET_MNEMONIC` and the `ARC_*` settings (`backend/README.md`). Without them every agent and challenge stays simulated exactly as above, `GET /chain` says `{"enabled": false}` and the vault endpoints answer **503** `chain_unavailable`.

Custody (D-57): each agent may be linked to one `AgentVault`, created and owned by the owner's browser wallet through our `VaultFactory`. The backend's operator key can only stake vault funds into an escrow challenge that names the vault, within the vault's onchain caps. Our own policy (`authorize`) still gates every stake first; the onchain caps are a second layer. Resolution remains **backend-administered**: the backend's resolver key decides the winner.

An agent linked to a vault has `env: "arc_testnet"`: its balances, ledger entries and challenges are in that env (its old simulated balance stays in the simulated ledger, unused). Challenges are only formed between two agents of the same env.

### Endpoints

| Method | Path | Body → Response | Notes |
|---|---|---|---|
| GET | `/chain` | → `ChainInfo` | Config for the web app. Never a key, never the backend's own RPC URL |
| GET | `/agents/{id}/vault` | → `VaultView` | 404 `not_found` (agent), 404 `vault_not_linked`, 503 `chain_unavailable` (chain disabled or RPC down) |
| POST | `/agents/{id}/vault` | `{tx_hash}` → `VaultView` | Links the vault created in `tx_hash`. Verified from the receipt: a `VaultCreated` log from **our** factory whose `agentRef` is this agent (UUID left-padded to 32 bytes) and whose operator is our operator, and the vault's current operator is still ours. Then the agent switches to `arc_testnet`. 422 `invalid_vault_tx` (bad hash, unknown / reverted tx, other factory, other agent, other operator; `message` says which), 409 `vault_already_linked`, 409 `agent_has_open_interactions` (open challenge or collaboration), 503 `chain_unavailable` |
| POST | `/agents/{id}/vault/sync` | `{tx_hash}` → `SyncReport` | Posts the USDC transfers into (`vault_deposit`) or out of (`vault_withdraw`) the vault found in that transaction's receipt. Transfers between the vault and the escrow (stakes, payouts) are the lifecycle's and are ignored here. Idempotent per log (`chain:{tx}:{log_index}`): replaying a tx credits nothing. A withdrawal larger than the agent's ledger available is **skipped** (listed in `skipped`, visible as `drift`), never forced. 404 `vault_not_linked`, 422 `invalid_vault_tx`, 503 `chain_unavailable` |
| POST | `/dev/chain/setup-demo-vaults` | `{amount?: money}` → `{env, owner, agents: [{id, link_tx, funding_tx, credited, vault: VaultView}], note}` | Dev only: mounted when dev endpoints are on **and** `AEN_DEV_CHAIN_OWNER=1`. Plays the owner with the test-owner key (HD index 2): for the seed-demo agents (`…00d1`, `…00d2`, run `/dev/seed-demo` first, else 409 `demo_not_seeded`) creates and links a vault if missing (caps = `amount`), tops it up to `amount` (default 5 USDC) and syncs the transfer, all through the same verification as the public endpoints. Renames "(simulated)" to "(Arc testnet)". 409 `dev_owner_tx_failed` if the test owner cannot pay |

```
ChainInfo  { enabled: true, chain_id: 5042002, chain_name: "Arc Testnet", rpc_url, explorer_url, usdc_address,
             usdc_decimals: 6, escrow_address, vault_factory_address, operator_address }
           | { enabled: false }
VaultView  { address, owner, operator,              // checksummed
             paused: bool, max_per_challenge: money, max_exposure: money,   // onchain caps
             onchain_balance: money,                 // USDC held by the vault (ERC-20 view, 6 decimals)
             onchain_exposure: money,                // its stakes in escrow not yet released
             ledger_available: money, ledger_committed: money,   // arc_testnet ledger
             drift: money (signed),                  // (onchain_balance + onchain_exposure) - (ledger_available + ledger_committed)
             explorer_url: string|null }             // {explorer}/address/{vault}
SyncReport { credited: LedgerEntry[],                // the agent's lines only: deposits positive, withdrawals negative
             skipped: [{ log_index, amount: money, reason: "insufficient_ledger_available" }],
             vault: VaultView }
```

`drift` is 0 when the ledger matches the vault. Positive = more onchain than in the ledger (usually a deposit not synced yet); negative = the ledger holds funds the vault no longer has (a skipped withdrawal).

### Challenge lifecycle on Arc testnet

Same states, same `authorize` checks, same ledger posting sets as simulated; what changes:

1. Commit (`accepted`/`awaiting_approval` → `funded`): both stakes are authorized and **reserved** in the ledger (`commit_stake`), and the challenge stops at `funded`.
2. Next phase of the same tick: the resolver registers the challenge in the escrow (`fundDeadline` = `start_at`, `resolveDeadline` = `end_at` + `RESOLUTION_GRACE_SECS`), the operator stakes each vault, and the challenge becomes `active` once the escrow is Active. If a vault cannot stake (paused, onchain cap, balance, operator revoked, revert) or the funding deadline passes, the escrow is voided onchain (`resolveVoid`), both reservations are refunded (`refund`) and the challenge is `cancelled` with `outcome: "void"`.
3. At `end_at` (wall clock; never earlier, whatever `/dev/tick`'s `now` says) the outcome is decided once from market data, then `resolve(winner vault)` / `resolveVoid` is sent, then the ledger settles (`settle` / `refund`, `tx_hash` = that transaction), then both vaults' exposure is released.
4. An RPC failure leaves everything as it was; the next tick reads the escrow first and continues. Nothing is sent or posted twice.

`arc_testnet` challenges use the wall clock everywhere; the simulated clock of `/dev/tick` applies to simulated challenges only. Tick reports may contain `chain_unavailable: …` errors (RPC down, or arc challenges waiting while the chain is not configured).

Activity kinds added: `vault_linked`, `vault_deposit`, `vault_withdraw`, `stake_reserved`, `escrow_created`, `stake_deposited`, `vault_released` (plus `stake_committed`, `challenge_settled`, `challenge_voided` with `detail.tx_hash` / `explorer_url` for arc). Amounts read `2.50 USDC (Arc testnet USDC)` in summaries.

## Approach and firms (simulated funds)

Design: `docs/design/approach-and-firms.md`. Decisions: D-64.

### Approach block (`approach.profile`)

A graph node with `kind: "approach"`, no inputs and no outputs, at most one per graph (`multiple_approaches`), exempt from the reachability and dead-end rules. Params (every field optional; the builder sends them complete, with explicit `null` bounds):

```
{ stance: { collaboration_bps: 0..10000 (default 3000), ambition_bps: 0..10000 (default 5000) },
  firm_role: "none"|"member"|"lead" (default "none"),
  markets: MarketFilter }
MarketFilter { scope: "any"|"structure"|"specific" (default "any"),
               structures: ("binary_up_down"|"price_ladder"|"categorical")[],
               categories: string[], subcategories: string[], keywords: string[], symbols: string[],  // ≤ 20 each, 1-64 chars
               ranges: { move_bps, yes_price_bps, expiry_secs, volume_delta_24h_bps: { min: int|null, max: int|null } } }
```

**Market filter semantics (binding).** `scope: "any"` ignores every filter. Otherwise:

- The *where* group: a market matches if its category is in `categories` **or** its subcategory (slug or name) is in `subcategories`; both empty = any. Case-insensitive.
- Every other non-empty group is ANDed with it and with each other: `structures` (one of), `keywords` (any keyword is a substring of the title, an event or instrument ticker, or a tag; case-insensitive), `symbols` (base asset; `btcusd` = `BTC`).
- Each set range bound is ANDed; bounds are inclusive. `move_bps` compares the absolute move. A market whose value is unknown never passes a bounded range.
- `min > max` and blank entries are 422.

Structure: Gemini `template` `crypto-up-down` = `binary_up_down`, `prices-ladder` = `price_ladder`, `type: categorical` = `categorical`, else unknown. Symbol: the leading letters of the event ticker (heuristic). Subcategory, tags and `volumeDelta24hPct` are read leniently (an unexpected shape leaves the field empty).

Effects (deterministic, never an LLM decision; `authorize` still applies to everything):

| Setting | Effect | Applies |
|---|---|---|
| `markets` | Discovery skips markets that do not match (activity `approach_filtered`, logged once while it persists). An incoming challenge on a non-matching market is rejected (`challenge_rejected` with `detail.reason: "outside_approach"`) | always (defaults match everything) |
| `collaboration_bps` = c | initiates challenges only if c ≤ 7000; joins firms on its own only if c ≥ 3000 and `firm_role = member`; may found a firm only if c ≥ 3000 and `firm_role = lead` | always |
| `ambition_bps` = a | stake capped at `max_per_interaction × (2000 + 8a/10) / 10000`; a proposal whose run conviction is below `7500 − a/4` is dropped (`approach_filtered`, `detail.reason: "min_conviction"`); no new proposal while `1 + 4a/10000` interactions are open | only when the graph has an approach node |

### Markets

| Method | Path | Body → result |
|---|---|---|
| GET | `/markets/categories` | → `{ categories: string[], subcategories: { <category>: string[] }, structures: string[], source: "gemini"\|"fake"\|"fallback" }`. Categories and subcategory names seen on active events (cached 5 min). If the listing fails: Gemini's top-level category list with `source: "fallback"` |
| POST | `/markets/preview` | `{ markets: MarketFilter, agent_id?: uuid }` → `{ count, sample: PreviewMarket[≤10], as_of, source, window_secs }`. Counts active, unresolved contracts matching now (one listing page of 100 events). The spot move is fetched only when `ranges.move_bps` is set, for up to 5 assets, over the agent's market-move window (`agent_id`) or the default. 422 `validation_failed` (paths `markets.ranges.<name>`, `markets.<list>`); 503 `market_data_unavailable` |

```
PreviewMarket { market_id, title, category, subcategory: string|null, structure: string|null, symbol: string|null,
                yes_price_bps: int|null /* bid/ask mid, else last trade (D-67) */, move_bps: int|null, expiry_ts: int|null, volume_delta_24h_bps: int|null, source }
```

### Firms

```
Firm { id, env: "simulated", name, mandate, lead_agent_id, lead_name,
       lead_share_bps: 5000..10000, member_pool_bps: 10000 - lead_share_bps,
       task_fee: money, roles: [{ role: "scout"|"analyst", slots: 1..5, weight_bps, filled: int }],
       markets: MarketFilter|null,          // null = the lead's approach markets
       effective_markets: MarketFilter,
       state: "open"|"closed",
       members: [{ agent_id, name, role, joined_at, left_at: ts|null, contributions: int, used: int, earned: money }],
       stats: { challenges: int, settled: int, lead_realized_pnl: money (signed), paid_to_members: money },
       actions: ("join"|"leave"|"close")[], created_at, closed_at: ts|null }
Contribution { id, firm_id, agent_id, agent_name, role, kind: "scout_candidate"|"analyst_signal",
               market_id, side: "above"|"below"|null, conviction_bps, interaction_id: string|null,
               used: bool, fee_paid: money, payout: money, summary, created_at }
Interaction  { …, firm_id: string|null }   // set on the lead's firm challenges; /interactions?agent_id=<lead> lists them
```

`members` lists every agent that has been a member (latest membership; `left_at` set once it left or the firm closed). `earned` = task fees + profit shares. `lead_realized_pnl` = the lead's settled P&L on firm challenges − `paid_to_members`. `actions`: `join` while a slot is free, `leave` while there are current members, `close`; none once closed.

| Method | Path | Body → result |
|---|---|---|
| GET | `/firms` | → `Firm[]`, newest first |
| POST | `/firms` | `{ lead_agent_id, name (1-80), mandate (1-500), lead_share_bps? (8000), task_fee? ("0"), roles? (scout 2 slots 6000, analyst 2 slots 4000), markets? }` → **201** `Firm`. 422 `validation_failed` (paths `name`, `mandate`, `lead_share_bps`, `roles`, `roles.<i>.slots`, `roles.<i>.weight_bps`, `roles.<i>.role`, `markets.…`; weights must add up to 10000); 422 `not_a_lead` (approach is not `firm_role: lead` with collaboration ≥ 30 %), `already_leads_firm`, `already_member`, `unsupported_env` (all with detail path `lead_agent_id`); 404 unknown lead (path `lead_agent_id`). A malformed body (e.g. a non-string `task_fee`) is the generic JSON rejection, without a field path |
| GET | `/firms/{id}` | → `Firm` |
| POST | `/firms/{id}/join` | `{ agent_id, role }` → `Firm`. Owner-driven: allowed whatever the agent's approach (the stance gates only autonomous joins). 409 `no_free_slot` (path `role`), `already_member` (path `agent_id`; a member of any firm), `invalid_state` (closed); 422 `is_lead`, `unsupported_env` (path `agent_id`) |
| POST | `/firms/{id}/leave` | `{ agent_id }` → `Firm`. 409 `not_a_member` (path `agent_id`) |
| POST | `/firms/{id}/close` | `{}` → `Firm`. Ends every membership; no new contributions; payouts on open firm challenges still settle. 409 `invalid_state` if already closed |
| GET | `/firms/{id}/contributions` | → `Contribution[]`, newest first |

Rules: a lead runs at most one open firm; an agent is a current member of at most one firm. Simulated agents only.

### Runtime (each tick)

1. **Autonomous joins** (before discovery): an active simulated agent with `firm_role: member`, collaboration ≥ 30 %, no firm, joins the open firm with a free slot whose markets overlap its own (a shared category, keyword or symbol, or either side `any`) and the highest member pool; within it, the free role with the highest weight (scout on ties). `firm_joined`.
2. **Scout**: when its own trigger fires on a market inside both the firm's and its own markets, it evaluates it and records one `scout_candidate` (at most one unused candidate per market per scout per hour). Scouts work without funds of their own.
3. **Lead**: before its own discovery it takes its scouts' best fresh (≤ 1 h), unused candidate (highest conviction) on a market it has no open challenge on, and runs its graph on it.
4. **Analysts**: when a lead's graph drafts a proposal, each current analyst evaluates the market with its own reasoner; the signal is its conviction for the lead's side. The lead proposes only if the mean signal ≥ its minimum conviction (the approach's, else `policy.min_conviction_bps`); otherwise `firm_draft_dropped` and nothing moves. Signals are recorded either way.
5. A contribution is **used** when it fed a challenge the lead actually proposed (`interaction_id` set; `Interaction.firm_id` set).

### Money (simulated ledger; balanced posting sets)

- **Task fee** (`firm_task_fee`, lead available → member available) per used contribution, gated by `authorize` as `PayTaskFee` by the lead: within `max_per_interaction` and the available balance, not counted as exposure, any interaction state. A fee above the lead's `approval_threshold` is not paid (fees have no approval flow). Denied: the contribution stays used, `fee_paid: "0"`, `firm_fee_denied`.
- **Profit share (binding)** (`firm_payout`, lead available → member available, one set per interaction) when a firm challenge settles with lead realized P&L `p > 0`: `pool = floor(p × member_pool_bps / 10000)`. The pool is divided **only among roles with at least one used contribution on that interaction**, in proportion to their `weight_bps` **renormalised over those contributing roles**, then **equally within a role**; every step floors (integer minor units) and the **remainder stays with the lead**. Example: p = 5 USDC, pool 1 USDC, one scout (6000) and one analyst (4000) used → 0.6 / 0.4 USDC; only scouts used → the scouts share the whole 1 USDC. Paid from the profit just credited. Not gated by `authorize` (contractual; paid even if the lead is paused).
- **Losses and voids** pay nothing; the lead bears losses alone.
- No double counting: the lead's `realized_pnl` is net of fees and shares paid; a member's receipts are `firm_income`, not P&L. Portfolio totals (same owner) are unchanged by these transfers.

### Activity kinds

All firm kinds carry `firm_id` (column and `detail.firm_id`); money-moving ones carry `detail.amount`: `firm_created`, `firm_joined`, `firm_left`, `firm_closed`, `firm_contribution` (`detail.kind`, `contribution_id`, `conviction_bps`), `firm_draft_dropped` (`detail.consensus_bps`, `min_conviction_bps`), `firm_task_fee` (`detail.amount`, `fee`, `contribution_id`), `firm_fee_denied` (`detail.fee`, `decision`), `firm_payout` (one per paid member with `detail.amount`, `payout`, `profit`, `pool`; one for the lead with `detail.amount` = total paid, `lead_keeps`). Without `firm_id`: `approach_filtered` (`detail.reason` or `detail.misses`).
