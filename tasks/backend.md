# Backend / Agent Runtime

- **Objective:** Rust core, API, runtime, integrations.
- **Scope:** backend/, schemas/. See docs/architecture.md §2-6, §8-9.
- **Dependencies:** Rust + Postgres installed; crate versions in docs/research/runtime-and-sandbox.md.
- **Assigned agent:** Backend / Agent Runtime specialist (branch `backend`)
- **Status:** M1 done (2026-11-14). M2 in progress on branch `backend-m2` (see "M2 log" at the end).

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

## M2 log (branch `backend-m2`, worktree `worktrees/backend-m2`)

Env for every command: `export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/postgresql@17/bin:$PATH" CARGO_TARGET_DIR=$HOME/dev/hackathongs/.cargo-target/backend-m2 TEST_DATABASE_URL="postgres://$USER@localhost/aen_m2_test"`; dev db `aen_m2_dev`. If a migration file is edited after being applied, `dropdb`/`createdb` the db.

Done (all committed):
- `core/src/challenge.rs`: `MarketRef`, `ChallengeTerms` (+`validate`), `FormulaId`, pure `resolve() -> A | B | Void`, exact `parse_decimal_scaled`.
- Security fixes from the lead's review: SEC-03 (commit/settle/refund ledger keys derived from the interaction; unique partial indexes `ledger_one_commit_per_interaction`, `ledger_one_release_per_interaction`; settlement under a row lock + state check), SEC-01 (`ApiJson` answers 415 unless `application/json`; bodiless POSTs such as approve/reject/tick take `{}`), SEC-05 (413). Merged `recovery` to get the adversarial tests; `state_changing_posts_must_require_a_json_content_type` is un-ignored; `settle_twice_...` now asserts rejection.
- `server/src/market/`: `MarketData` trait, `discover` / `observe` built on it, deterministic `FakeMarket`, `GeminiClient` (reqwest, 5 s timeout, 3 attempts with doubling backoff).
- `server/src/reasoner.rs`: `Reasoner` trait, strict `ReasonerOutput` validation, `RuleBasedReasoner`.
- `server/src/graph_exec.rs`: walks a validated graph, emits `PlannedIntent`s (an `Intent` + terms), stores a trace.
- `server/src/lifecycle.rs`: `tick` (expire, discover, respond, commit, resolve), `try_commit`, `decide_approval`, `settle_locked`. Migration `0003_interactions.sql`.
- `server/src/interactions.rs` + routes: `/interactions`, `/interactions/{id}`, `/approvals`, approve / reject, `/stats/portfolio`, `/dev/tick`, `/dev/fake-market`. Interval scheduler in `main.rs` (`TICK_INTERVAL_SECS`, 0 disables).
- Tests: `cargo test --workspace` green (core 94 + 14 + 6 + 2; server 22 unit, 11 + 5 adversarial, 17 api, 25 lifecycle).

- Docs: `docs/api.md` (M2 section), `docs/research/gemini-prediction-markets.md` (live spike), `backend/README.md`, `.env.example`.
- Gemini client tested against a local mock HTTP server (parse, retry with backoff, timeout, 404 not retried, bad identifiers). One manual tick against the live public API (simulated stakes) proposed on `GEMI-BTC2611202200-HI64500` and `GEMI-BTC2611160000-HI65100`.

### Tests

`cd backend && cargo test --workspace` -> **211 passed, 0 failed, 0 ignored** (core: 94 unit, 14 + 6 adversarial, 2 schema; server: 37 unit, 11 + 5 adversarial, 17 api, 25 lifecycle). `cargo fmt` and `cargo clippy --all-targets` clean. No test uses the network.

### Curl-driven lifecycle (2026-11-15, `MARKET_DATA=fake TICK_INTERVAL_SECS=0`, db `aen_m2_dev`, script in `backend/README.md`)

| Step | Result |
|---|---|
| deposit 200, allocate 100 + 100 | bull 100 / 0, skeptic 100 / 0 (available / committed, USDC) |
| tick 1 | `proposed 1, countered 1`; state `negotiating`, stake 5.00 -> 2.50, turn = bull |
| tick 2 | `accepted 1, funded 1`; state `active`; each agent 97.50 / 2.50; wallet committed 5.00 |
| fake YES price -> 0.70, tick at `end_at + 5 s` | `resolving 1, settled 1`; outcome `a` (bull), observed 7000 vs threshold 5500 |
| end | bull 102.50 / 0, realized +2.50, 1 win; skeptic 97.50 / 0, realized -2.50; wallet committed 0; portfolio realized 0 |
| ledger rows for the interaction | 4 `commit_stake` + 6 `settle`; escrow nets to 0; replayed tick adds none |
| activity | proposed, countered, accepted, 2x stake_committed, 2x settled, all full sentences |

### Financial and behavioural defaults chosen in M2 (for `docs/decisions.md`; all on simulated funds, owner may overturn)

| # | Default | Where (single place to change) |
|---|---|---|
| F-1 | A challenge is a claim on one integer observation. "Above" wins when `observed >= threshold`, "below" when `<` (Gemini's "or above" strike convention). No ties | `core/src/challenge.rs` `resolve` |
| F-2 | Formulas: `pm_yes_price_bps_v1` = contract `resolutionSide` (yes 10000 / no 0) if settled, else `lastTradePrice` in bps; `spot_last_price_micros_v1` = spot `last`. Observation = first successful read at or after `end_ts` (not the price exactly at `end_ts`) | `core/src/challenge.rs` `FormulaId`, `server/src/market/mod.rs` `observe` |
| F-3 | Missing data is never a value: retry every tick for `RESOLUTION_GRACE_SECS` (600) after `end_ts`, then **void**: both stakes refunded, state `cancelled`, `outcome: void`, no P&L | `lifecycle.rs` `settle_locked` |
| F-4 | Equal stakes; winner takes both (+stake / -stake); no fee | `core/src/ledger.rs` `settle` |
| F-5 | `start_ts` is the **funding deadline** (proposal time + `FUNDING_WINDOW_SECS`, 300). Not funded by then (still negotiating or awaiting approval) = cancelled, approvals expire. The challenge is active as soon as both stakes commit; `end_ts = start_ts + agent horizon_secs` | `reasoner.rs` `GenerateProposal`, `lifecycle.rs` `expire_one` |
| F-6 | Authorization happens three times: at proposal, at each answer, and at commit. Only the commit can end in an approval request. `RequireApproval` during negotiation lets the talk continue; the stake still cannot move without the owner | `lifecycle.rs` |
| F-7 | Commit authorizes **both** agents against their **current** policy version and a fresh snapshot, under locks. Any deny cancels the whole challenge; nobody is funded alone | `lifecycle.rs` `try_commit` |
| F-8 | An approval is per (interaction, agent, stake). Approving re-runs authorization; a deny then cancels (HTTP 200 with the interaction `cancelled`). Rejecting cancels. `action.request_approval` in a graph forces an approval request even under the threshold (it can only tighten) | `lifecycle.rs` `try_commit`, `decide_approval` |
| F-9 | Exposure = stakes of the agent's challenges in funded / active / resolving. Unfunded agreements do not count | `lifecycle.rs` `AGENT_SQL` |
| F-10 | Negotiation: at most `MAX_NEGOTIATION_ROUNDS` (3) offers including the proposal; one more counter cancels. A counter may change only stake and threshold. A denied answer, or a flow that ends without an action, cancels | `lifecycle.rs` `respond_one`, `reasoner.rs` `validate` |
| F-11 | Settlement and refunds are contract execution, not agent intents: they are not re-authorized, so a paused or stop-lossed agent's open challenge still settles | `lifecycle.rs` `settle_locked` |
| F-12 | Rule-based reasoner: conviction `5000 + 5 x spot move in bps` (contrarian if the strategy text says so), clamped 500-9500; proposes at the current market value; stake = approval threshold (max per interaction at >= 8000 bps conviction), halved for low risk tolerance, capped by available; counter = half the stake | `reasoner.rs` |
| F-13 | Counterparty = the oldest other active agent with a `trigger.challenge_received` flow and no open challenge on that market. An agent holds one open challenge per market. An agent without that flow answers counters with a built-in flow: accept at or above `policy.min_conviction_bps`, else reject | `lifecycle.rs` `pick_counterparty`, `graph_exec.rs` `default_response_graph` |
| F-14 | Market choice: the open Gemini prediction-market contract on the trigger's asset priced closest to 0.50 (within 0.05-0.95), in the soonest event expiring after the funding deadline; else spot price, labelled with the reason | `market/mod.rs` `discover` |
| F-15 | Negotiation reasons over the market snapshot taken at proposal; resolution always reads the market again | `lifecycle.rs` `respond_one` |

### Deviations from `docs/architecture.md`

- Terms use an absolute integer `threshold` in the formula's unit, not `threshold_bps`; the market is a `MarketRef`, not an `asset` symbol (needed for prediction-market contracts, D-14).
- The graph executor returns `PlannedIntent { intent, terms, ask_owner, summary }`, not a bare `Vec<Intent>`: `core::Intent` carries only the stake and stays the thing `authorize()` judges; terms are data, validated separately.
- No `Funded` dwell: `Accepted -> Funded -> Active` happen in one transaction (simulated funds, no deposit confirmation to wait for).
- Every POST now needs a JSON content type and body (`{}` for approve / reject / tick) because of SEC-01. **Frontend: `web/lib/api/client.ts` only sets the header when there is a body, so it must send `{}` on those calls.**
- New dependency: `reqwest` (rustls) for the Gemini client and the Grok reasoner.
- Grok: the model returns an opinion plus stake and threshold only; terms are assembled in Rust from the request (it cannot name a market).

### Known limitations / unfinished

- `GrokReasoner` (`server/src/grok.rs`) exists and is used when `XAI_API_KEY` is set, but has **only been tested against a local stand-in server**: no real xAI call was made (no key). The raw `/chat/completions` path and model name `grok-4.7` come from `sponsors.md` and are unverified against the live API. It falls back to the rules on any error; a run's `reasoner` field still says `grok` then, and the summary starts with "(Rule-based fallback". Co-captain not started.
- No cooldown: after a cancelled negotiation or a settlement the same agents may propose again on the next tick (bounded by exposure, balance and stop-loss, but noisy with the 15 s scheduler).
- `trigger.challenge_received` is not event-driven: answers happen on the next tick.
- Only `resolutionSide: "yes"` was observed live; `"no"` is assumed symmetric (see the research file).
- The simulated clock (`/dev/tick` `now`) writes real `ts` on ledger and activity rows; only interaction timestamps follow it.
- Collaboration interactions, `arc_testnet` funds and `/webhooks/quicknode` are not started (chain work is blocked on D-3).
- `/dev/*` is mounted by default and unauthenticated like everything else (D-9); the server binds loopback only.

### Next steps

1. Try `GrokReasoner` once with a real `XAI_API_KEY` (and D-8 approval); adjust the endpoint or schema if xAI answers differently.
2. A proposal cooldown per (agent, market).
3. Frontend: interactions list / detail, approvals inbox, portfolio charts from `/stats/portfolio`.
4. Lead: copy F-1..F-15 into `docs/decisions.md`; fill `docs/demo.md` from the README script.

## M2b (2026-11-15, branch `backend-m2b`)

Status: done. `cargo test --workspace` -> **222 passed** (core 94 + 14 + 6 + 4 schema; server 40 unit, 11 + 5 adversarial, 17 api, 31 lifecycle). `cargo fmt --check` and `cargo clippy --workspace --all-targets` clean. Test db `aen_m2b_test` (all server tests, including the adversarial ones, read `TEST_DATABASE_URL`; there is no separate variable, so `aen_m2b_sec` was not needed). Demo script run by hand on `aen_m2b_dev`, port 8091.

| Task | What changed | Where |
|---|---|---|
| 1 Cooldown | An agent does not propose again on a market for `PROPOSAL_COOLDOWN_SECS` after its last proposal there. Checked before reasoning (no run, no model call) and again in the proposing transaction | `lifecycle.rs` `in_proposal_cooldown`, `state.rs` |
| 2 Summaries | Rule-based summaries and graph traces write `2.50 USDC (simulated)`, `0.55 (YES price)`, `65%`, `+3%`. Structured fields unchanged. Grok's prompt asks for the same (unverified against the live model) | `reasoner.rs`, `graph_exec.rs`, `models.rs` `display_bps`, `grok.rs` |
| 3a Approvals | Root cause below; no rule changed. The demo skeptic's threshold makes the approval step deterministic | `demo.rs`, test `a_proposer_sizes_its_stake_into_the_automatic_range` |
| 3b Clock jumps | Root cause below; no rule changed. `/dev/tick` refuses a simulated `now` that would expire unfunded challenges unless `allow_expiry: true` | `interactions.rs` `dev_tick` |
| 4 Schemas | `schemas/challenge_terms.schema.json` (+ `Money` as a digit string); up-to-date test and a wire-format test | `core/src/challenge.rs`, `core/src/money.rs`, `core/tests/schemas.rs` |
| 5 Demo seed | `POST /api/dev/seed-demo` `{}`: two fixed-id agents from the starter templates, 100 simulated USDC each, idempotent. Lifecycle test `seeded_demo_walks_the_whole_lifecycle`. README script and `docs/api.md` updated | `demo.rs`, `agents.rs` `insert_agent` |

### Root causes (QA M2 observations 2 and 3)

- **Lowered `approval_threshold` rarely asks for approval.** By design (F-12): `reasoner.rs` `proposal_stake` sizes a *proposer's* stake to `min(approval_threshold, max_per_interaction)` and only uses the per-challenge maximum from 80% conviction (move >= 600 bps with the rule-based reasoner). `authorize` asks the owner only when `stake > approval_threshold`, so the stake follows the threshold down and stays automatic. The gate is correct; what surprised QA is the sizing. A *counterparty* takes the stake on the table, so its threshold does trigger approvals. Kept as is; the demo gets its approval from the skeptic (threshold 1.00, agreed stake 2.50). The "Momentum bull" graph's `cond.stake_lte` max (5.00) is a graph parameter and does not follow the policy; harmless (it can only add approvals).
- **Clock jumps cancel negotiations.** By design (F-5): `start_ts` = proposal + `FUNDING_WINDOW_SECS` (300) is the funding deadline and the first phase of every tick expires unfunded challenges past it. A simulated `now` more than 5 minutes ahead is past that deadline. Not a bug; the dev endpoint now says so instead of silently expiring.

### New defaults for docs/decisions.md

| # | Default | Rationale | Where |
|---|---|---|---|
| F-16 | Proposal cooldown: 600 s per (proposing agent, market), counted from the proposal's `created_at`, regardless of how that challenge ended. `PROPOSAL_COOLDOWN_SECS`, `0` disables. Being a counterparty starts no cooldown | Stops the propose / cancel loop on every 15 s tick. Longer than the 300 s funding window so an expired challenge is not re-proposed in the tick that expires it. Counted from the proposal, not the end, so it is a plain rate limit and does not depend on the simulated clock used to resolve | `state.rs`, `lifecycle.rs` `in_proposal_cooldown` |
| F-17 | Dev clock guard: `/dev/tick` with `now` at or past the funding deadline of an unfunded challenge is 409 unless `allow_expiry: true`. The scheduler and the deadline rule (F-5) are unchanged | Demo reliability without weakening the rule | `interactions.rs` `dev_tick` |
| F-18 | Demo seed: fixed agent ids `…00d1` / `…00d2`, 100 simulated USDC each (one 200 deposit, idempotency keys `demo-seed:*`), skeptic `approval_threshold` 1.00 USDC, everything else from the templates. Existing demo agents are never modified | Deterministic walk; goes through the normal validation and ledger paths | `demo.rs` |

### Known limitations

- The seeded walk is deterministic only when the demo agents are the only active agents (others may propose or be picked as counterparty).
- Re-running the demo within the cooldown needs `PROPOSAL_COOLDOWN_SECS=0` or a wait.
- A proposal that is denied or finds no counterparty creates no interaction, so it is not rate-limited by the cooldown (activity is still logged once; with Grok this is one model call per tick).
- Approvals are decided at real time, not the simulated clock.

## Co-captain (2026-11-15, branch `cocaptain`, worktree `worktrees/cocaptain`)

Status: done (text suggestions, backend only). `cargo test --workspace` -> **254 passed** (was 222; server unit 40 -> 59, new `tests/suggestions.rs` 13). `cargo fmt --check` and `cargo clippy --workspace --all-targets` clean. Test db `aen_cc_test`. Checked by hand on `aen_cc_dev`, port 8092: seed demo, pause, tick -> 1 suggestion, 415 without JSON, dismiss, accept -> 409, generate -> nothing new.

| What | Where |
|---|---|
| `CoCaptain` trait, `Facts`, typed `ProposedChange`, `Draft::vet`, six rules with formulas in doc comments | `server/src/cocaptain.rs` |
| Facts from ledger / approvals / interactions / activity, storage, expiry, `GET /suggestions`, `POST /suggestions/generate`, `/accept`, `/dismiss` | `server/src/suggestions.rs`, `lib.rs` |
| `suggestions` table: one pending per (agent, kind), a subject is never repeated, decided rows immutable, no deletes | `migrations/0004_suggestions.sql` |
| `agents::apply_update` / `current_input` extracted from `PUT /agents/{id}`; accept calls it | `server/src/agents.rs` |
| Rules run at the end of every tick (`TickReport.suggestions`); no model call on ticks | `server/src/lifecycle.rs` |
| `GrokCoCaptain` when `XAI_API_KEY` is set; rules are the floor, fallback on any error | `server/src/grok_cocaptain.rs`, `main.rs`, `state.rs` |

Boundaries kept: the co-captain receives `Facts` and returns drafts; it has no pool, ledger or `authorize` handle. Accept re-parses the stored change into the closed patch type, merges it into the current version and runs the normal agent validation in one transaction; any failure is a 422 and the suggestion stays pending. Tests assert the ledger, approvals and interaction states are byte-for-byte unchanged across generate / accept / dismiss.

Test files now truncate `suggestions` too (it references `agents`); a branch that adds a test file with its own `TRUNCATE ... agents` list must include it.

### New defaults for docs/decisions.md

| # | Default | Rationale | Where |
|---|---|---|---|
| F-19 | `approval_expiring` warns within 120 s of the funding deadline | The funding window is 300 s and ticks are 15 s apart: late enough not to nag on every approval, early enough to act | `cocaptain.rs` `APPROVAL_EXPIRY_WARN_SECS` |
| F-20 | `idle_funds`: idle = 3600 s without any interaction or config change with >= 1 USDC available and nothing open; unfunded = zero balance while >= 1 USDC is unallocated | One hour is several cooldown periods (600 s); 1 USDC ignores dust | `IDLE_SECS`, `IDLE_MIN_FUNDS` |
| F-21 | `lower_max_stake`: 3 straight settled losses, or realized loss >= 20% of net allocation; proposes halving `max_per_interaction` (threshold follows down if needed) | Tightening only, so cheap to accept; halving is easy to explain and to undo | `LOSING_STREAK`, `DRAWDOWN_BPS` |
| F-22 | `repeated_denials`: 3 denials for one reason under the current version; raise is to the largest blocked stake (capped at the aggregate limit) or aggregate + that stake | The smallest change that would have let the blocked stakes through; counted per version so an edit resets it | `DENIAL_REPEAT` |
| F-23 | `raise_approval_threshold`: last 3 owner decisions under the current version all approvals; new threshold = largest approved stake, capped at the per-challenge limit | Never suggests more than the owner already approved by hand | `APPROVAL_STREAK` |
| F-24 | `loosens_control` is computed by the server (higher limit, threshold or stop-loss) and such suggestions say so in the body; a suggestion can set a stop-loss but not remove one, and cannot touch the graph, interaction types, name or status | The owner must see when accepting weakens a control (AGENTS.md 8.2) | `ProposedChange::loosens` |
| F-25 | De-duplication by subject: a dismissed or accepted suggestion is not repeated until its subject changes (new approval, new settle count, new version, new pause); a pending rule suggestion whose condition is gone becomes `expired` (a fourth status) | Ticks run every 15 s; without this a dismissed suggestion would return at once. `expired` keeps "the owner dismissed it" distinct from "it stopped applying" | `suggestions.rs` `insert`, `expire_stale` |
| F-26 | Grok is called only by `POST /suggestions/generate`, never on ticks; its items replace the rule item of the same agent and kind, anything else is `general` | Cost, and the rules stay the floor so a model omission cannot hide a warning | `grok_cocaptain.rs` |

### Known limitations / unverified

- `GrokCoCaptain` has **only been tested against a local stand-in server**; endpoint path and model name are the same unverified ones as `GrokReasoner`. The prompt contains agent names, policies and numbers, never keys.
- `idle_funds` compares the tick's clock with `agents.updated_at` (always real time), so a simulated `now` far ahead makes agents look idle.
- Denials are counted from activity rows; the lifecycle logs a persisting identical denial once (`record_once`), so three denials can take longer than three ticks.
- Accepting a change equal to the agent's current values is a 422 (`invalid_proposed_change`); the owner dismisses it instead.
- No frontend yet; voice (ElevenLabs) not started.


## M3 stage 1 (2026-11-16, branch `backend-m3`, worktree `worktrees/backend-m3`)

Status: done. Tests (two db groups): `aen_m3_test` -> core 94 + 14 + 6 + 4, server unit 59, api 17, lifecycle 33 (+2), suggestions 13; `aen_m3_sec` -> adversarial_api 11, adversarial_integrity 5. **256 passed.** fmt and clippy clean. README demo (incl. the new co-captain step) run verbatim on `aen_m3_dev`, port 8093.

| Task | What changed | Where |
|---|---|---|
| 1a Blocked proposals rate-limited | A discovery run that `authorize` denied or that found no counterparty is marked (`agent_runs.blocked`, `market_key`) and starts the proposal cooldown for that market, only under the same agent version. Checked before reasoning, so no run / model call | `migrations/0005_blocked_proposals.sql`, `lifecycle.rs` `in_proposal_cooldown`, `mark_blocked` |
| 1b P&L clock | `pnl_over_time.ts` = `interactions.settled_at` (new column, the tick's `now`, simulated in dev), ordered by it; ledger `ts` unchanged | `interactions.rs` `portfolio`, `lifecycle.rs` `settle_locked` |
| 2 Co-captain demo step | No new endpoint: two more seeded rounds (`/dev/fake-market`, `/dev/tick`, approve) -> skeptic's third loss -> `lower_max_stake` `{"policy":{"max_per_interaction":"5000000"}}` (plus `raise_approval_threshold`); accept -> version 2 | README "Co-captain step", `tests/lifecycle.rs` `seeded_demo_walks_the_whole_lifecycle` |

API changes (`docs/api.md`): cooldown note extended; `pnl_over_time` row in Metrics. No new endpoints or fields.

### New defaults (proposals for docs/decisions.md)

| # | Default | Rationale | Where |
|---|---|---|---|
| D-38 | A proposal attempt denied by `authorize` or without a counterparty starts the same `PROPOSAL_COOLDOWN_SECS` (600 s) cooldown per (agent, market) as a proposal, counted from the run's `ts` (lifecycle clock), only while the agent's config version is unchanged. Reasoner errors and invalid terms are not rate-limited (a transient error should retry). Supersedes the "not rate-limited yet" note in D-33 | Stops a blocked agent re-reasoning (and calling Grok) every 15 s tick; an owner edit is the usual fix for a denial, so it should take effect on the next tick | `lifecycle.rs` |
| D-39 | P&L over time is plotted against the settlement time on the lifecycle clock (`interactions.settled_at`), not the ledger rows' wall-clock `ts`; rows settled before migration 0005 use `observed_at` (same value) | Charts of a simulated-clock demo would otherwise bunch every point at the moment the script ran | `interactions.rs` |

### Known limitations

- `no_counterparty` is rate-limited even if a new agent appears during the cooldown (it is picked up after at most 600 s).
- Approvals are still decided at real time, so in the co-captain step the stakes are committed at wall-clock time while proposals and settlements use the simulated clock.
