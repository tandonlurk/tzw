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
(default `http://localhost:3000`), `RUST_LOG` (default `info,tower_http=debug`).
See `.env.example`; the server does not read `.env` itself, so export the
variables (`set -a; source .env; set +a`).

## Test

```sh
cargo test --workspace
```

`core` tests need nothing. The `server` integration tests use
`postgres://$USER@localhost/aen_test` (override with `TEST_DATABASE_URL`); they
run migrations and **truncate every table except templates**, so never
point them at a database you care about.

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
  `PROPOSAL_COOLDOWN_SECS` (600) after its last proposal. Wait, or start the
  server with `PROPOSAL_COOLDOWN_SECS=0`.

With `MARKET_DATA=gemini` the same flow runs on live Gemini prediction-market
contracts (public data, no key) with simulated stakes, in real time: `now`
cannot be overridden and a challenge lasts the agent's `horizon_secs`.

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
- No auth (single demo user), so the server binds to loopback only.
