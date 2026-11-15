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
run migrations and **truncate the agent, ledger and activity tables**, so never
point them at a database you care about.

Before committing: `cargo fmt && cargo clippy --all-targets && cargo test --workspace`.

## Schemas

`../schemas/graph.schema.json` and `../schemas/blocks.json` are generated from
`core`. Regenerate after changing the graph types or the block catalog:

```sh
cargo run -p aen-core --example export_schemas
```

A test fails if the checked-in files are out of date.

## Notes

- All funds are **simulated** in M1. Nothing here touches a chain or real money.
- Money is integer USDC minor units (6 decimals), sent as strings.
- Ledger rows and agent versions are append-only (enforced by DB triggers).
  Balances and stats are always computed from the ledger.
- No auth (single demo user), so the server binds to loopback only.
