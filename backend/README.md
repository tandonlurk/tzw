# Backend

Rust workspace. `crates/core` is pure domain logic (money, ledger rules, policy,
`authorize`, interaction state machine, behavior-graph catalog and validator).
`crates/server` is the Axum + SQLx API described in `../docs/api.md`.

## Requirements

- Rust stable (1.97+).
- PostgreSQL 17 running locally with databases `aen_dev` and `aen_test`
  (`createdb aen_dev && createdb aen_test`).

## Run

```sh
cd backend
export DATABASE_URL="postgres://$USER@localhost/aen_dev"
cargo run -p aen-server          # http://127.0.0.1:8080/api, migrations run on startup
curl http://127.0.0.1:8080/api/health
```

Environment: `DATABASE_URL` (required), `PORT` (default 8080), `CORS_ORIGIN`
(default `http://localhost:3000`), `RUST_LOG` (default `info,tower_http=debug`),
`BIND_ADDR` (default `127.0.0.1`), `AEN_API_TOKEN`, `ALLOWED_HOSTS` (request
guard, see below and `docs/api.md`).
See `.env.example`; the server does not read `.env` itself, so export the
variables (`set -a; source .env; set +a`).

## Test

```sh
cargo test --workspace
```

`core` tests need nothing. The `server` integration tests use
`postgres://$USER@localhost/aen_test` (override with `TEST_DATABASE_URL`); they
run migrations and **truncate every table except templates** (through
`tests/common/mod.rs`, the only code that lifts the SEC-08 TRUNCATE guard), so
never point them at a database you care about.

Before committing: `cargo fmt && cargo clippy --all-targets && cargo test --workspace`.

## Challenge lifecycle (M2)

Agents discover an opportunity, propose, negotiate, are authorized, commit
stakes, and the challenge resolves and settles. Simulated funds only; nothing
signs or sends a transaction. The moving parts:

| Module | Role |
|---|---|
| `core::challenge` | terms, formulas, pure `resolve() -> A | B | Void` |
| `server::market` | `MarketData` trait; `gemini` (Gemini **crypto exchange** public API) and `fake` |
| `server::reasoner`, `server::grok` | `Reasoner` trait, output validation, `RuleBasedReasoner`; `GrokReasoner` (xAI) when `XAI_API_KEY` is set, falling back to the rules on any error |
| `server::graph_exec` | walks an agent's graph, returns intents + a trace; has no DB or ledger handle |
| `server::lifecycle` | `tick`: every intent goes through `core::authorize` in the transaction that applies it |

Extra environment (all optional, see `.env.example`): `MARKET_DATA` (`gemini`
default, or `fake`), `TICK_INTERVAL_SECS` (default 15, `0` disables the
scheduler), `MAX_NEGOTIATION_ROUNDS` (3), `FUNDING_WINDOW_SECS` (300),
`RESOLUTION_GRACE_SECS` (600), `PROPOSAL_COOLDOWN_SECS` (600, per agent and
market, `0` disables), `DEFAULT_SYMBOL` (`BTCUSD`), `DEV_ENDPOINTS`
(`true`), `GEMINI_BASE_URL`.

### Demo script (offline, simulated funds)

Deterministic on a fresh database (other active agents would also take part).
Needs `jq`. All funds are simulated; nothing signs or sends a transaction.

```sh
createdb aen_demo
DATABASE_URL="postgres://$USER@localhost/aen_demo" PORT=8099 MARKET_DATA=fake TICK_INTERVAL_SECS=0 \
  cargo run -p aen-server &
API=http://127.0.0.1:8099/api
post() { curl -sS -X POST "$API$1" -H 'Content-Type: application/json' -d "$2"; }

post /dev/seed-demo '{}'   # "Demo Bull (simulated)" + "Demo Skeptic (simulated)", 100 simulated USDC each; safe to repeat
post /dev/fake-market '{"move_bps":300,"yes_price_bps":5500,"resolution_side":"open"}'   # BTC +3%, contract at 0.55
post /dev/tick '{}'        # proposed 1, countered 1: bull offers 5.00, skeptic counters 2.50 -> negotiating
post /dev/tick '{}'        # accepted 1, approvals_requested 1 -> awaiting_approval, nothing funded
APPROVAL=$(curl -sS $API/approvals | jq -r '.[0].id')
post /approvals/$APPROVAL/approve '{}' | jq .interaction.state    # "active": both stakes committed together
ID=$(curl -sS $API/interactions | jq -r '.[0].id')
AFTER_END=$(curl -sS $API/interactions/$ID | jq -r '.end_at | fromdateiso8601 + 5 | todateiso8601')
post /dev/fake-market '{"yes_price_bps":7000}'   # the contract now trades at 0.70
post /dev/tick "{\"now\":\"$AFTER_END\"}"         # simulated clock: resolving 1, settled 1
curl -sS $API/interactions/$ID | jq '{state, outcome, observed, negotiation: [.negotiation[].summary]}'
curl -sS $API/stats/portfolio | jq .agents     # bull +2.50, skeptic -2.50 (simulated)
```

**Co-captain step** (continue in the same shell). Two more identical rounds on
the simulated clock, each won by the bull. After the skeptic's third straight
loss the rule-based co-captain (no model call) suggests halving its maximum
stake; accepting it is an owner edit and creates config version 2.

```sh
at() { jq -rn --arg t "$1" --argjson d "$2" '$t | fromdateiso8601 + $d | todateiso8601'; }
round() {   # one round starting at simulated time $1; leaves its settle time in $NOW
  post /dev/fake-market '{"yes_price_bps":5500}' >/dev/null
  post /dev/tick "{\"now\":\"$1\"}" | jq -c '{proposed, countered}'                     # {"proposed":1,"countered":1}
  post /dev/tick "{\"now\":\"$(at $1 15)\"}" | jq -c '{accepted, approvals_requested}'  # {"accepted":1,"approvals_requested":1}
  post /approvals/$(curl -sS $API/approvals | jq -r '.[0].id')/approve '{}' | jq -r .interaction.state   # active
  post /dev/fake-market '{"yes_price_bps":7000}' >/dev/null
  NOW=$(at "$(curl -sS $API/interactions | jq -r '[.[] | select(.state == "active")][0].end_at')" 5)
  post /dev/tick "{\"now\":\"$NOW\"}" | jq -c '{settled, suggestions}'
}
round $AFTER_END   # ... {"settled":1,"suggestions":0}   (two losses: no suggestion yet)
round $NOW         # ... {"settled":1,"suggestions":2}   (third loss)
curl -sS "$API/suggestions?status=pending" | jq -c '.[] | {agent_name, kind, proposed_change}'
# {"agent_name":"Demo Skeptic (simulated)","kind":"raise_approval_threshold","proposed_change":{"policy":{"approval_threshold":"2500000"}}}
# {"agent_name":"Demo Skeptic (simulated)","kind":"lower_max_stake","proposed_change":{"policy":{"max_per_interaction":"5000000"}}}
SKEPTIC=00000000-0000-4000-8000-0000000000d2
SUG=$(curl -sS "$API/suggestions?status=pending" | jq -r ".[] | select(.kind == \"lower_max_stake\" and .agent_id == \"$SKEPTIC\") | .id")
post /suggestions/$SUG/accept '{}' | jq .status     # "accepted"
curl -sS $API/agents/$SKEPTIC | jq -c '{version, max_per_interaction: .policy.max_per_interaction}'
# {"version":2,"max_per_interaction":"5000000"}
curl -sS $API/stats/portfolio | jq -c '.agents[] | {name, realized_pnl, settled, wins}'
# bull 7500000 / 3 / 3, skeptic -7500000 / 3 / 0 (simulated); pnl_over_time.ts are the simulated settle times
```

The suggestion body reads: `"Demo Skeptic (simulated)" lost its last 3
challenges (realized P&L -7.50 USDC (simulated)). Halving its maximum stake per
challenge from 10.00 USDC (simulated) to 5.00 USDC (simulated) limits further
losses while you review its strategy.` The second suggestion
(`raise_approval_threshold`, F-23) appears because the owner approved the
skeptic's last three stakes by hand; it loosens a control and says so.

Things that surprise people (all intended, see `tasks/backend.md`):

- **Why the skeptic asks for approval.** Its automatic limit is 1.00 USDC and the
  agreed stake is 2.50. A *proposer* sizes its own stake into its automatic range
  (at most `approval_threshold`, the per-challenge maximum only from 80%
  conviction), so lowering a proposer's threshold shrinks its stakes instead of
  producing approvals. The counterparty takes the stake on the table.
- **The funding deadline is real.** A challenge must be funded within
  `FUNDING_WINDOW_SECS` (300) of the proposal, approvals included. `/dev/tick`
  refuses a simulated `now` past the deadline of an unfunded challenge (409
  `clock_jump_would_expire`) unless `"allow_expiry": true`. For a slow live demo
  start the server with a longer `FUNDING_WINDOW_SECS`.
- **Running it again.** The bull will not propose on the same market for
  `PROPOSAL_COOLDOWN_SECS` (600) after its last proposal, nor after an attempt
  that was denied or found no counterparty (until the agent is edited). Wait,
  or start the server with `PROPOSAL_COOLDOWN_SECS=0`.

With `MARKET_DATA=gemini` the same flow runs on live Gemini prediction-market
contracts (public data, no key) with simulated stakes, in real time: `now`
cannot be overridden and a challenge lasts the agent's `horizon_secs`.

## Co-captain suggestions

`server::cocaptain` (`CoCaptain` trait, the six deterministic rules and their
formulas), `server::suggestions` (facts read from the ledger, approvals,
interactions and activity; storage; accept / dismiss) and
`server::grok_cocaptain` (xAI, only when `XAI_API_KEY` is set, falls back to
the rules on any error; **unverified against the live API**, mock-tested
only). The co-captain is advisory: it is handed facts and returns text. It
has no path to `authorize`, the executor or the ledger; accepting a
suggestion with a proposed change calls `agents::apply_update`, the same
validated, versioned edit as `PUT /agents/{id}`.

```sh
B=http://127.0.0.1:8080/api; J='content-type: application/json'
curl -s -X POST $B/agents/$AGENT/status -H "$J" -d '{"status":"paused"}'
curl -s -X POST $B/suggestions/generate -H "$J" -d '{}'       # or wait for a tick
curl -s "$B/suggestions?status=pending"
curl -s -X POST $B/suggestions/$ID/accept -H "$J" -d '{}'      # or /dismiss
```

Rule suggestions are also generated at the end of every tick (no model call
there). Endpoints and thresholds: `docs/api.md`.

## Schemas

`../schemas/graph.schema.json`, `../schemas/challenge_terms.schema.json` and
`../schemas/blocks.json` are generated from `core`. Regenerate after changing
the graph types, the challenge terms or the block catalog:

```sh
cargo run -p aen-core --example export_schemas
```

A test fails if the checked-in files are out of date.

## Notes

- All funds are **simulated** (M1 and M2). Nothing here touches a chain or real money.
- Money is integer USDC minor units (6 decimals), sent as strings.
- Ledger rows and agent versions are append-only (enforced by DB triggers).
  Balances and stats are always computed from the ledger.
- **Request guard (SEC-02, `server::guard`).** Always on: Host allowlist
  (`localhost`, `127.0.0.1`, `[::1]`, plus `ALLOWED_HOSTS`) against DNS
  rebinding, and any `Origin` other than `CORS_ORIGIN` is refused (CSRF). A
  bearer token is required whenever `AEN_API_TOKEN` is set, and the server
  refuses to start on a non-loopback `BIND_ADDR` without one. The web app does
  not send a token yet, so token mode is for scripted / remote use; the local
  default needs none.
- **Database guards (SEC-08).** `TRUNCATE` is refused on the append-only tables
  (`ledger_entries`, `agent_versions`, `negotiation_rounds`, `suggestions`)
  unless the transaction runs `SET LOCAL aen.allow_truncate = 'on'`, which only
  the test harness (`tests/common/mod.rs`) does. A deferred constraint trigger
  refuses any posting set (env + idempotency key) that does not sum to zero at
  commit. To wipe a dev database: `psql aen_dev -c "BEGIN; SET LOCAL
  aen.allow_truncate = 'on'; TRUNCATE suggestions, activity, ledger_entries,
  agent_runs, approvals, negotiation_rounds, interactions, agent_versions,
  agents; COMMIT;"` (or drop and recreate it).
- **Least-privilege role (recommended, not enforced).** A superuser or table
  owner can drop these triggers. For anything shared, run migrations as the
  owner and the server as a role with only `SELECT, INSERT` on
  `ledger_entries`, `agent_versions`, `negotiation_rounds`, `activity` and
  `SELECT, INSERT, UPDATE` on the rest (no `DELETE`, no `TRUNCATE`). Today the server runs
  migrations at startup with its own connection, so this needs a separate
  migrate step first; left as a default for review (D-proposal in
  `tasks/backend.md`).
