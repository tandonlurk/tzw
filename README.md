# Autonomous Agent Economic Network

User-owned AI agents that discover disagreements on prediction-market questions, negotiate measurable challenges, commit funds inside owner-set limits, and build an auditable track record. Hackathon project (UBC 2026). Product and process context: `AGENTS.md`; live status: `TODO.md`.

**Everything runs on simulated funds today.** Nothing signs or sends a chain transaction; the escrow contract is tested locally but not deployed (custody decision D-3 is pending). Resolution is backend-administered.

## Layout

| Path | What |
|---|---|
| `backend/` | Rust: `core` (money, ledger, `authorize`, state machine, graph schema) and `server` (Axum API, runtime, Gemini client, co-captain) |
| `web/` | Next.js app: dashboard, agents, builder, wallet, activity, approvals, co-captain |
| `contracts/` | Foundry: `ChallengeEscrow.sol` + tests |
| `schemas/` | JSON Schemas exported from `core` |
| `docs/` | architecture, decisions (D-n), API, demo, research, security, submission drafts |

## Setup (macOS, no Docker)

Prerequisites: Rust stable, Foundry, PostgreSQL 17 (Homebrew service), Node 22.

```sh
export PATH="$HOME/.cargo/bin:$HOME/.foundry/bin:/opt/homebrew/opt/postgresql@17/bin:$PATH"
createdb aen_dev && createdb aen_test && createdb aen_sec_test
git submodule update --init
```

Environment: copy `backend/.env.example` and `web/.env.example` to `.env` (never commit them) and export the backend one (`set -a; source backend/.env; set +a`); the server does not read `.env` itself. No keys are needed for the simulated demo.

## Run

```sh
# API on http://127.0.0.1:8080/api (migrations run on startup)
cd backend && DATABASE_URL="postgres://$USER@localhost/aen_dev" cargo run -p aen-server
# Web on http://localhost:3000
cd web && npm ci && npm run dev
```

Demo (deterministic, simulated): fresh `aen_dev`, start the API with `MARKET_DATA=fake TICK_INTERVAL_SECS=0`, then follow "Demo script" in `backend/README.md`. Expected numbers: `docs/demo.md`.

## Test

```sh
cd backend && cargo test --workspace     # uses aen_test and aen_sec_test; truncates them
cd contracts && forge test
cd web && npm test && npm run build
```

## Status and limits

See `TODO.md` ("Needs you" lists open owner decisions) and `docs/submission/` for an honest per-sponsor account of what is live, mock-tested, simulated or not built.
