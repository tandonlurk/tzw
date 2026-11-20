# Backend / Agent Runtime

## Grok Bot mock adversary (2026-11-20)

- Objective/scope: practice-only external Bot integration; assigned backend specialist in isolated worktree, lead reviewed and integrated as `8c38667` on `integration-1119`. Status: locally implemented and tested; live Bot round pending owner setup.
- Files: migration `0015_sparring.sql`, `src/sparring.rs`, guard/state/router/startup configuration, `tests/sparring.rs`, shared test wipe list, README and `.env.example`.
- Completed: immutable agent-version snapshots, append-only rounds, owner-only replies and restricted Bot-only adversary submissions; distinct token fails closed, Host/Origin retained, strict text/URL validation and concurrent 12-round cap. No ledger/configuration/live lifecycle effects.
- Tests: `cargo check -p aen-server --offline` passed; four PostgreSQL integration tests passed on isolated `aen_sparring_test`; `cargo test -p aen-server --lib --offline` 84 passed, 0 failed, 1 ignored (live Gemini). Format/diff clean. Sandbox initially denied local PostgreSQL and mock HTTP bindings; approved unrestricted reruns passed. Owner demo DB untouched.
- Decision/assumptions: D-75; single-owner API model retained, Bot source label identifies credential use rather than verified model authorship. No automatic rebuttal, RL training, backtest or strategy activation.
- Next/blocker: owner must sign into actual Grok Bot, configure public HTTPS endpoint and separate restricted token through secure handoff, then record a real critique. Setup: `docs/setup/grok-bot-sparring.md`; full handoff: `reports/handoffs/2026-11-20-grok-adversary.md`.

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

## M3 item 3: SEC-02, SEC-08, SEC-09 (2026-11-16, branch `backend-m3`)

Status: done. Tests: `aen_m3_test` -> core 94 + 14 + 6 + 4, server unit 63 (+4 guard), api 17, lifecycle 33, suggestions 13; `aen_m3_sec` -> adversarial_api 16 (+5), adversarial_integrity 6 (+1, and one extended). **266 passed.** The 7 repro tests were run against the code without the fixes (guard layer off, old `ledger_db.rs`, a fresh db without migration 0006) and all failed there. fmt and clippy clean. Checked by hand on port 8093: default config serves curl and a `localhost:3000` browser-style request, refuses `Host: evil.example` and `Origin: https://evil.example` (403); `BIND_ADDR=0.0.0.0` without a token and a short token refuse to start; with a token `/health` 200, `/wallet` 401 without and 200 with it. Migration 0006 applied cleanly to a dev db with existing demo data.

| Item | What | Where |
|---|---|---|
| SEC-02 | Guard middleware (inside CORS): Host allowlist, foreign `Origin` refused on every method, bearer token when `AEN_API_TOKEN` is set (constant-time compare, redacted `Debug`, >= 32 chars). `BIND_ADDR` (default loopback); non-loopback without token = startup error | `src/guard.rs`, `lib.rs`, `main.rs`, `state.rs` (`Settings.guard`) |
| SEC-08 | `BEFORE TRUNCATE` statement trigger on the 4 append-only tables, opt-out `SET LOCAL aen.allow_truncate = 'on'` used only by `tests/common/mod.rs::wipe` (all test files now use it); deferred constraint trigger: every (env, key) set sums to zero at commit. Least-privilege role documented, not enforced | `migrations/0006_ledger_hardening.sql`, `tests/common/mod.rs` |
| SEC-09 | Unique `(env, idempotency_key, line)`; replay lookup per env and compares `interaction_id`; client keys stored as `client:owner:{key}` so they cannot pre-claim server keys (`settle:…`, `demo-seed:…`) | `ledger_db.rs`, migration 0006 |

API changes (`docs/api.md`): new "Request guard" and "Idempotency" paragraphs (403 `host_not_allowed`, 403 `origin_not_allowed`, 401 `unauthorized`); no endpoint changes. Env: `BIND_ADDR`, `AEN_API_TOKEN`, `ALLOWED_HOSTS` (`.env.example`).

### New defaults (proposals for docs/decisions.md)

| # | Default | Rationale | Where |
|---|---|---|---|
| D-42 | Request guard always on: hosts `localhost`/`127.0.0.1`/`[::1]` + `ALLOWED_HOSTS`; a request without Host passes (browsers always send one, so DNS rebinding is still covered); any `Origin` other than `CORS_ORIGIN` is 403 on every method; no `Origin` passes | Blocks DNS rebinding and CSRF with no change to the local web app or curl |
| D-43 | Bearer token (`AEN_API_TOKEN`, >= 32 chars) required on all routes except `GET /api/health` and `OPTIONS` when set; required to start on a non-loopback `BIND_ADDR`. One shared token = the single owner; the web app does not send it yet | Simplest auth that makes a non-loopback bind safe; per-user auth is out of MVP scope |
| D-44 | DB refuses TRUNCATE on append-only tables unless `SET LOCAL aen.allow_truncate = 'on'` (test harness only), and refuses unbalanced posting sets at commit. Least-privilege role recommended (README) but not enforced, because the server runs its own migrations | Defense in depth against bugs and accidents; not against a superuser |
| D-45 | Idempotency keys unique per env; client keys namespaced `client:{caller}:` (caller = `owner` today); replay must match account, delta, reason and `interaction_id` | A client could pre-claim a server key and block a settlement or the demo seed |

### Notes

- Old client keys in existing dev databases stay un-namespaced (harmless; new ones are prefixed).
- `adversarial_integrity` CSRF test now expects 403 for a foreign Origin (refused before the 415 check) and still checks 415 for the allowed origin.


## sandbox-host stage 1 (2026-11-16, branch `sandbox-host`, worktree `worktrees/sandbox-host`)

Status: done (stage 1). The D-11 spike is now crate `backend/crates/sandbox` (`aen-sandbox`); no runtime wiring yet (stage 2: `custom.code` block -> `Sandbox::run_blocking` -> `core::authorize`).

- **API:** `Sandbox::new(path)` (compiles once, `module_sha256()`), `default_module_path()` (`$AEN_SANDBOX_MODULE` or `crates/sandbox/guest/runner.wasm`), `with_limits`, `with_max_concurrency` (default 4), sync `run(code, &ctx, now_ms, seed)` / `run_with_limits`, async `run_blocking` (`spawn_blocking` behind a `Semaphore`), `stats()`. Returns `RunOutput { intents: Vec<aen_core::authorize::Intent>, logs, fuel_used }` or `SandboxError` (Display is audit-trail text; Javy `unreachable` -> `Unsupported`).
- **Guest:** `./backend/crates/sandbox/build-guest.sh` (checksum-verified Javy v9.1.0 in gitignored `tools/`); `guest/runner.wasm` is gitignored. Javy output is not byte-reproducible (two builds of the same source gave different hashes), so the recorded SHA-256 identifies the built artifact, not the source.
- **Tests:** `cargo test -p aen-sandbox`: 3 unit + 24 integration passed, 1 ignored (latency). Tests needing the module skip (message visible with `--nocapture`) when it is absent. fmt and clippy clean; `cargo build --workspace` OK.
- **Fixed vs spike:** `{"type":"reject","stake":"1"}` was accepted (serde ignores extra fields on an internally tagged unit variant); `Reject {}` is now a struct variant in the mirror. Money must also be canonical (no leading zeros), as in `core::Money`.
- **Debug-build cost:** without `[profile.dev.package."*"] opt-level = 3` at the workspace root, compiling the guest takes ~8 s in a dev build (0.6 s optimised). Full run p50 ~0.3 ms, ~1.05M fuel. Lead's call: add the profile line, or cache with `Module::serialize`.

## M3 item 4: collaboration MVP (2026-11-16, branch `backend-m3`)

Status: done. Tests: `aen_m3_test` -> core 99 (+5: split, transitions, terms, payout set), adversarial_authorize 14, adversarial_ledger 6, schemas 4, server unit 63, api 17, lifecycle 33, suggestions 13, **collaborations 10 (new)**; `aen_m3_sec` -> adversarial_api 16, adversarial_integrity 6. **281 passed** (sandbox crate tests also pass, untouched). fmt and `clippy --workspace --all-targets` clean. Checked by hand on `aen_m3_dev`, port 8093: seed, propose, accept, fund -> 409 `approval_required`, fund with approval, complete (+1.80 / -1.80), second complete -> 409, activity readable.

| What | Where |
|---|---|
| States, transitions, terms validation, `split` (floor for proposer, remainder to partner) | `core/src/collaboration.rs` |
| Ledger reasons `collab_commit` / `collab_payout` (via `escrow:{collab}`) / `collab_refund`, `PostingSet::for_collaboration` | `core/src/ledger.rs` |
| Table `collaborations`; `collaboration_id` on `activity` and `ledger_entries`; unique indexes: one commit and one release per (collaboration, account) | `migrations/0007_collaborations.sql` |
| REST: list/get/create/accept/reject/fund/complete/cancel, `actions` for the UI, activity per party | `server/src/collaborations.rs`, `lib.rs` |
| Funded contributions count toward aggregate exposure | `lifecycle.rs` `AGENT_SQL` |
| `?collaboration_id=` on `/activity`; `collaboration_id` on `ActivityItem` and `LedgerEntry` | `misc.rs`, `models.rs`, `ledger_db.rs` |

No new `core::Intent` variant: the sandbox's exhaustive Intent mirror (D-41) would have to change, and that crate is not mine. Accept is authorized as `ProposeCollaboration` of the partner's contribution (state checked separately), reject as `Reject`, funding as `CommitFunds { collaboration }`.

### New defaults (proposals for docs/decisions.md)

| # | Default | Rationale | Where |
|---|---|---|---|
| D-46 | Collaborations are owner-driven over REST (no agent reasoning yet). Proposal, acceptance (as `ProposeCollaboration` of the partner's contribution), rejection (`Reject`) and funding (`CommitFunds`, both sides, fresh snapshots, one transaction) go through `authorize`; a contribution above `approval_threshold` needs `owner_approved: true` on `/fund` (409 `approval_required` otherwise). Payouts and refunds release funded contributions by the agreed terms and are not gated again, like challenge settlement and refund. Completion is owner-declared | Every commitment of funds is gated; nothing funded can be trapped by a later pause or policy edit |
| D-47 | Both contributions > 0. Split: proposer gets `floor(pool × bps / 10000)`, partner the remainder; zero shares get no posting. Contributions sit in each agent's `committed` while funded and count toward `max_aggregate_exposure`. Collaboration results are not in `realized_pnl` (challenge P&L); shown per collaboration as `Party.net` | Exact integer accounting; keeps the challenge metrics and co-captain rules unchanged |
| D-48 | Seeded demo agents allow `collaboration` as well as `challenge` (new seeds only; existing agents untouched) | So a fresh demo can show collaborations; the challenge walk is unchanged |

### Known limitations

- No agent-initiated collaborations (no graph block / reasoner path yet) and no approvals inbox entry for them: approval is the `owner_approved` flag on `/fund`.
- If the sandbox owner adds collaboration-specific intents to the mirror, accept / complete / cancel can switch to them.

## M3 item 5: QuickNode Streams ingestion, local only (2026-11-16, branch `backend-m3`)

Status: done. Tests: `aen_m3_test` -> core 99, adversarial_authorize 14, adversarial_ledger 6, schemas 4, server unit 66 (+3), api 17, lifecycle 33, suggestions 13, collaborations 10, **quicknode 8 (new, fixture-driven)**; `aen_m3_sec` -> adversarial_api 16, adversarial_integrity 6. **292 passed** (backend crates; sandbox untouched). fmt and `clippy --workspace --all-targets` clean. Checked by hand on port 8093 with `openssl`-signed fixture: 200 inserted 3, replay 200 duplicates 3, bad signature 401.

| What | Where |
|---|---|
| `POST /api/webhooks/quicknode`, mounted only with `QUICKNODE_STREAMS_SECRET`; HMAC-SHA256 over `nonce + timestamp + body` (constant-time `verify_slice`), timestamp window, JSON only, per-log validation, insert `ON CONFLICT (tx_hash, log_index) DO NOTHING`, reorg `removed` marking, activity (`chain_event`, `chain_event_removed`, `chain_events` summary over 50) | `server/src/quicknode.rs`, `lib.rs`, `state.rs`, `main.rs` |
| `chain_events` table | `migrations/0008_chain_events.sql` |
| Guard: webhook path exempt from the bearer token (HMAC is its auth); any extra `ALLOWED_HOSTS` entry now requires `AEN_API_TOKEN` at startup | `guard.rs` |
| Fixture + tests | `tests/fixtures/quicknode/logs_batch.json`, `tests/quicknode.rs` |
| Deps `hmac 0.13`, `sha2 0.11` (already in the lock via sqlx); the lock gained `const-oid 0.10.3` (digest feature) | `server/Cargo.toml`, `Cargo.lock` |

### New defaults (proposals for docs/decisions.md)

| # | Default | Rationale | Where |
|---|---|---|---|
| D-49 | QuickNode webhook assumptions (UNVERIFIED, configurable): headers `x-qn-signature` / `x-qn-timestamp` / `x-qn-nonce`, signature = hex HMAC-SHA256(secret, nonce + timestamp + body), timestamp window 300 s (seconds or ms). Payload `{data, metadata}` with `eth_getLogs`-format logs, flat or per block. Invalid logs are skipped and counted; non-2xx only for auth / shape errors | Matches the research notes; everything that is a guess can be changed by env without code |
| D-50 | Events are stored once per `(tx_hash, log_index)`; a later `removed: true` marks the row removed (never deleted). Nothing acts on events (no executor), and `agent_id` is null in their activity until contracts and interactions are linked | Duplicate / reordered / concurrent deliveries are no-ops; audit trail kept |
| D-51 | The webhook path is exempt from the bearer token; Host and Origin checks still apply. Any extra `ALLOWED_HOSTS` entry requires `AEN_API_TOKEN` at startup, because a tunnel to the loopback port exposes every route | QuickNode cannot send our token; the HMAC is its authentication, and the rest of the API stays closed |

### Known limitations

- Header names, signed-message layout and the logs payload are unverified: capture one real delivery (Filter Editor `console.log`) before relying on it.
- No gzip support (configure the stream uncompressed); default 2 MB body limit.
- Events are not decoded or linked to interactions / agents yet; there is no chain executor.


## sandbox-host stage 2: `custom.code` in the runtime (2026-11-16, branch `sandbox-host`)

Status: done. Tests (own dbs): `aen_sh_test` -> core 100 + 14 + 6 + 4, sandbox 3 + 24 (+1 ignored), server unit 72, api 17, collaborations 10, custom_code 6 (new), lifecycle 33, quicknode 8, suggestions 13; `aen_sh_sec` -> adversarial_api 16, adversarial_integrity 6. **332 passed, 1 ignored (latency).** fmt and clippy clean. README "Custom code" demo run verbatim on a fresh `aen_sh_dev`, port 8096.

| What | Where |
|---|---|
| `custom.code` block (kind `action`, input any, `source` ≤ 64 KiB bytes); schemas exported (16 blocks) | `core/src/graph.rs`, `schemas/blocks.json` |
| One `Sandbox` per process in `AppState.sandbox` (`None` if the guest is missing; logged with its SHA-256) | `server/src/custom_code.rs` `shared_sandbox`, `state.rs`, `main.rs` |
| Executor: `ctx` -> `run_blocking` -> planned intents -> existing validation -> `authorize`; failures fail closed as `custom_code_failed`; runs record `reasoner = "custom_code"` | `graph_exec.rs` `run_code`, `plan_code_intent`; `lifecycle.rs` error paths |
| `POST /agents/{id}/code/test-run` (writes nothing), dry run on save (422) | `custom_code.rs` `test_run`, `check_on_save`; `agents.rs` |
| Dev compile of the guest 8.2 s -> 0.6 s (server boot measured 0.61 s): `opt-level = 3` for the Cranelift / wasmtime crates only | `backend/Cargo.toml` |

### New defaults (proposals for docs/decisions.md)

| # | Default | Rationale | Where |
|---|---|---|---|
| D-52 | Custom code chooses *whether* and *how much*, not the claim: a `propose_challenge` becomes the rule-based template terms (side, threshold, deadlines; shown to the code as `ctx.proposal`) with the code's stake. Answers: `accept_challenge` must use the stake on the table, `counter_offer` changes only the stake, `reject`. `commit_funds` and `propose_collaboration` are refused. Any unusable intent fails the whole run (no partial plans) | Keeps `core::Intent` unchanged (D-41) and every claim measurable; extending the output with direction/threshold is a later, reviewable change |
| D-53 | `ctx` = trigger, `now_ms`, agent id/name/version/config, balances (available, exposure), market view, proposal template, incoming terms + own side. No policy limits, keys, addresses or other agents. Seed = first 4 bytes of SHA-256(agent id, now_ms, trigger) | Enough to write a strategy; nothing that helps probe or bypass limits; runs replay exactly |
| D-54 | Save runs each source once on a sample `ctx` (`market.source = "sample"`, sample challenge for answering flows); any sandbox error is a 422. Skipped (save allowed) when the server has no sandbox. Explicitly not a security boundary: a test saves code that behaves only on the sample | Catches typos and throws early; the runtime limits are what protect the tick |
| D-55 | Custom-code failures are recorded like reasoner errors: `agent_runs.status = error`, activity `custom_code_failed` via `record_once`, not rate-limited (D-38). A responder whose code fails leaves the challenge open to retry until the funding deadline | Consistent with existing failure handling; a persistent failure costs ~7 ms CPU per tick and is logged once |
| D-56 | Test-run judges each intent as it would be used (proposal: new interaction; answer: state `proposed`; commit: `accepted`) against the agent's real balances and the current market, and writes nothing | The owner sees the verdict the tick would give, with no side effects |

### Known limitations

- The concurrency semaphore (4) is shared by ticks, test-runs and save checks; a burst of saves can make a tick wait up to ~0.5 s per queued run.
- A graph with more than one `custom.code` node on one path runs each one; nothing caps that beyond the 64-step graph limit.
- Javy builds are not byte-reproducible, so the recorded module SHA-256 identifies the built file, not the source (stage 1 note).

## Log: aen-chain crate (branch chain-vaults, 2026-11-16, D-57..D-59)
- `crates/chain`: alloy 2.6 client. Resolver (idx 0) create/resolve/resolveVoid; operator (idx 1) vault stake/release; receipt verification (`vaults_created`, `usdc_transfers`); `OwnerWallet` for the test owner (idx 2); `testkit` feature (anvil + deploy from `contracts/out`).
- Refuses chains other than 5042002/31337; checks escrow `RESOLVER()` is our key and the factory is bound to our escrow. Simple nonce manager (D-59).
- Tests: `cargo test -p aen-chain` 2 passed (anvil lifecycle, foreign resolver refused). Skips if anvil missing; needs `forge build`.
- Not done: server integration. Plan: `reports/handoffs/2026-11-17-chain-vaults.md` steps 2–5.

## chain-wiring: Arc testnet in the lifecycle (branch `chain-wiring`, 2026-11-17)

- Objective: chain-vaults handoff steps 2-5 (lifecycle wiring, endpoints, dev autonomous runs, anvil E2E). Assigned: backend specialist. Status: done, awaiting lead review.
- Files: `core/src/ledger.rs` (`VaultDeposit`/`VaultWithdraw`, `chain:{tx}:{log}` keys, `PostingSet::with_tx_hash`); migration `0009_chain_wiring.sql`; `server/src/chain.rs` (config from env, `GET /chain`, vault link/view/sync, dev setup-demo-vaults); `server/src/lifecycle_arc.rs` (fund / resolve / release phases); `lifecycle.rs` (arc commit stops at `funded`, wall clock for arc, settle split into `decide_outcome` + `apply_outcome`, env-labelled amounts); `ledger_db.rs` (writes `tx_hash`); `agents.rs`/`models.rs` (`vault_address`, `vault_owner`); `interactions.rs` (`escrow` tx hashes); `main.rs`, `state.rs` (`AppState.chain`); `tests/chain.rs`.
- Tests: `cargo test --workspace` 341 passed, 0 failed, 1 ignored (6 new anvil tests in `tests/chain.rs`, 1 new core test). Clippy clean. Manual E2E: real server binary + local anvil + `setup-demo-vaults` + demo ticks, settled onchain, drift 0 (README "Autonomous demo").
- Decisions proposed: `reports/handoffs/2026-11-17-chain-wiring.md`.
- Not done: live Arc run (lead), QuickNode as event source, browser-wallet withdraw sync is manual (UI posts the tx).

## approach-firms-backend: approach block and firms (branch `approach-firms-backend`, 2026-11-17)

- Objective: backend for `docs/design/approach-and-firms.md` plus `TickReport.activated`. Assigned: backend specialist. Status: done, awaiting lead review.
- Files: `core/src/approach.rs` (filter, stance/ambition functions), `core/src/firm.rs` (terms, `split_profit`, analyst gate, `choose_firm`), `core/src/graph.rs` (`approach.profile`, `BlockKind::Approach`, `multiple_approaches`, `param_path`), `core/src/authorize.rs` (`Intent::PayTaskFee`), `core/src/ledger.rs` (`firm_task_fee`, `firm_payout`); migration `0010_firms.sql`; `server/src/firms.rs` (API + runtime), `server/src/markets.rs` (`/markets/categories`, `/markets/preview`), `lifecycle.rs` (gates, scout/lead/analyst flow, auto-join phase, profit share on settle, `activated`), `lifecycle_arc.rs`, `market/*` (listing metadata), `agents.rs` (`firm_income`, P&L net of firm payments), `interactions.rs` (`firm_id`), `misc.rs` (`?firm_id=`); `tests/firms.rs`.
- Tests: `cargo test --workspace` all passed (incl. anvil `tests/chain.rs` with the new `activated` asserts); 10 new integration tests, new core unit tests. Clippy clean. Manual E2E: server binary on the fake market, auto-join + scout + analyst + firm proposal + task fees over HTTP.
- Decisions: D-64. Docs: `docs/api.md` "Approach and firms".
- Not done: starter templates have no approach node (would change demo numbers); firms on Arc testnet; `/markets/preview` reads one 100-event page and computes spot moves for ≤ 5 assets.
