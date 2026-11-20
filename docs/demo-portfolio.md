# Populated simulated portfolio for recording

`scripts/seed-video-demo.py` builds a synthetic demo scenario through the normal HTTP API. It does not insert ledger rows, performance points, or financial outcomes directly. All balances, challenges, charts, and transaction history remain labeled simulated. The scenario is not investment performance or real network activity.

## Dataset

- 6,000 simulated USDC total, funded as twelve 500-USDC simulated deposits.
- 1,500 unallocated; 4,500 distributed across three strategy families.
- Atlas Momentum: 2 instances total, 750 USDC initial allocation each.
- Vega Contrarian: 3 instances total, 500 USDC initial allocation each.
- Lumen Cautious: 5 instances total, 300 USDC initial allocation each.
- The multipliers are actual clones with stored family/variance metadata, not leverage. Members of a family cannot challenge each other.
- Twelve historical batches over the preceding two days generate actual discovery, negotiation, authorization, commitment and settlement against deterministic fake-market observations. The goal is at least 30 settled challenges and a populated two-day P&L chart.
- A final current-time batch leaves active challenges for the recording. Automatic scheduler stays paused, so recording does not create unattended activity.

Three strategies plus their clones appear as ten ordinary agents. They retain their strategy differences: momentum discovery, contrarian incoming-challenge responses, and cautious scheduled discovery with smaller stakes. The single-trigger graph constraint is preserved. All history stakes fit independently enforced automatic permissions.

## Run on a fresh isolated backend

Keep the owner's existing database unchanged. Create a new database such as `aen_video_demo_20261120`, and start the API on a separate local port using:

```sh
export DATABASE_URL="postgres://$USER@localhost/aen_video_demo_20261120"
export MARKET_DATA=fake TICK_INTERVAL_SECS=0 PROPOSAL_COOLDOWN_SECS=0
export FUNDING_WINDOW_SECS=300 BIND_ADDR=127.0.0.1 PORT=8090
unset AEN_TESTNET_MNEMONIC XAI_API_KEY
# Start the current backend binary/build with the normal request-guard token.
```

In another shell, load the general API token from the local environment without printing it, then:

```sh
export DEMO_API_URL=http://127.0.0.1:8090/api
python3 scripts/seed-video-demo.py
```

The script reads `AEN_API_TOKEN` from the environment; never pass secrets as arguments, paste them into chat, or commit `.env`. Point the web app's server-side `AEN_API_URL` at this backend after checking the result; leave browser API requests going through `/backend` when using the protected proxy.

## Safety and replay behavior

Before modifying anything, the script refuses preexisting agents, financial balances, or interactions and verifies the fake-market dev endpoint exists. It rejects nonsimulated agents, HTTP endpoints outside loopback, credentials in the URL, and redirects. It uses idempotency keys for deposit/allocation calls. Multiplication is not retried blindly.

A successful rerun recognizes the scenario marker, validates the wallet/family sizes/history, prints the existing summary, and makes no mutations. A partially completed run is deliberately refused: use another fresh isolated database rather than rerunning financial or clone creation on uncertain state. No reset/delete operation is included.

The summary checks that `unallocated + allocated = 6000 USDC`, gives family sizes, settled/open challenge counts, and transaction counts. Allocations and withdrawals are transfers, not P&L. Aggregate realized P&L remains zero for the closed simulated challenges because the same owner owns both sides and no fees are charged.

## Recording timing

History is seeded using past simulated timestamps and deterministic fake observations. It never advances beyond the recording session's present time. The current active batch has roughly a ten-minute horizon including its funding window. To settle it after it ends, change the fake observation and trigger a dev tick; the existing lifecycle computes the result. If you leave the server running with the scheduler off, active challenges remain unchanged until a manual tick.

## Verification

Python syntax checked using `ast.parse` without generating build artifacts. Actual HTTP run results are recorded in the task handoff after the isolated backend is available. No claims of actual API execution or expected transaction counts should be made from syntax validation alone.
