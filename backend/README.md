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
| `server::reasoner` | `Reasoner` trait, output validation, `RuleBasedReasoner` |
| `server::graph_exec` | walks an agent's graph, returns intents + a trace; has no DB or ledger handle |
| `server::lifecycle` | `tick`: every intent goes through `core::authorize` in the transaction that applies it |

Extra environment (all optional, see `.env.example`): `MARKET_DATA` (`gemini`
default, or `fake`), `TICK_INTERVAL_SECS` (default 15, `0` disables the
scheduler), `MAX_NEGOTIATION_ROUNDS` (3), `FUNDING_WINDOW_SECS` (300),
`RESOLUTION_GRACE_SECS` (600), `DEFAULT_SYMBOL` (`BTCUSD`), `DEV_ENDPOINTS`
(`true`), `GEMINI_BASE_URL`.

Offline demo, driven by hand (needs `jq`):

```sh
createdb aen_demo
DATABASE_URL="postgres://$USER@localhost/aen_demo" PORT=8099 MARKET_DATA=fake TICK_INTERVAL_SECS=0 \
  cargo run -p aen-server &
API=http://127.0.0.1:8099/api
post() { curl -sS -X POST "$API$1" -H 'Content-Type: application/json' -d "$2"; }
agent_from() { curl -sS $API/templates | jq -c --arg n "$1" '.[] | select(.name==$n) | {name, description, config, policy, graph}' \
  | curl -sS -X POST $API/agents -H 'Content-Type: application/json' -d @- | jq -r .id; }

post /wallet/simulated-deposit '{"amount":"200000000"}'
BULL=$(agent_from "Momentum bull"); SKEPTIC=$(agent_from "Skeptic")
post /agents/$BULL/allocate '{"amount":"100000000"}'; post /agents/$SKEPTIC/allocate '{"amount":"100000000"}'
post /dev/tick '{}'        # bull proposes 5 USDC, skeptic counters with 2.5
post /dev/tick '{}'        # bull accepts; both stakes authorized and committed -> active
ID=$(curl -sS $API/interactions | jq -r '.[0].id')
post /dev/fake-market '{"yes_price_bps":7000}'                     # the fake contract trades at 0.70
post /dev/tick '{"now":"<a time after the interaction end_at>"}'   # resolves and settles: bull +2.5, skeptic -2.5
curl -sS $API/interactions/$ID | jq '{state, outcome, observed, negotiation, runs}'
curl -sS $API/stats/portfolio | jq .
```

With `MARKET_DATA=gemini` the same flow runs on live Gemini prediction-market
contracts (public data, no key) with simulated stakes, in real time: `now`
cannot be overridden and a challenge lasts the agent's `horizon_secs`.

## Schemas

`../schemas/graph.schema.json` and `../schemas/blocks.json` are generated from
`core`. Regenerate after changing the graph types or the block catalog:

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
