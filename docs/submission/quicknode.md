# QuickNode Streams (draft)

Criteria as published: "QuickNode Streams must be a core data source."

## Intended role

Streams (dataset `Logs`, webhook destination) on Arc testnet delivers the escrow contract's events (created, deposited, resolved, refunded) to the backend, which turns them into the agents' activity history and, once a chain executor exists, confirms funding and settlement from them rather than from our own transaction submissions. Arc testnet is a listed Streams network (`docs/research/sponsors.md` section 2).

## What is built (local only)

- `POST /api/webhooks/quicknode` (`backend/crates/server/src/quicknode.rs`), mounted only when `QUICKNODE_STREAMS_SECRET` is set.
- HMAC-SHA256 check with constant-time compare and a 300 s timestamp window; the header names are configurable because QuickNode's are not documented.
- Idempotent storage keyed on (transaction hash, log index): duplicate, reordered and concurrent deliveries store each log once; a reorg (`removed: true`) is recorded once.
- Events appear as readable activity entries. Fixture-driven tests (`backend/crates/server/tests/quicknode.rs`).

## Honest status

- **No stream is configured and the route is not exposed.** It needs a QuickNode account, a deployed escrow contract on Arc testnet (blocked on D-3 custody) and a public tunnel (owner decision; the API then requires its bearer token on every other route).
- The signature header names, the signed-message layout and the payload fields are **UNVERIFIED assumptions** (`docs/api.md`, "QuickNode Streams webhook"). Capture one real delivery and adjust before relying on it.
- Nothing acts on the events yet: the simulated lifecycle does not depend on them. Until the escrow is deployed, Streams is not yet a core data source in practice, which is what the bounty asks for.
