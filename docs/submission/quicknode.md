# QuickNode Streams

Criteria as published: "QuickNode Streams must be a core data source."

## What Streams does in TZW

Every Arc testnet challenge moves testnet USDC through our `ChallengeEscrow`: the escrow is registered, each agent's vault deposits its stake, the escrow activates, then pays the winner (or voids). A QuickNode Stream (`tzw-arc-testnet-escrow`: Arc Testnet, dataset Logs, webhook, server-side JS filter on our escrow and vault-factory addresses) pushes each of those events to the backend about one second after its block.

The backend decodes each event, finds the challenge it belongs to (the escrow id is the challenge id) and checks it against what the backend itself confirmed from its own transaction receipts: same transaction, same two vaults, same amounts, same winner. The result is what the owner sees:

- On the challenge page, every escrow step shows "confirmed · block 73114369 · 1.0 s" next to its transaction, and a "Confirmed by QuickNode Streams" table lists each event with its block, latency and explorer link.
- In Activity: "Escrow funded on Arc: "Demo Bull"'s vault deposited 0.50 USDC (Arc testnet USDC), confirmed by QuickNode Streams (block 73114369, 1.0 s after the block)." with a "via QuickNode Streams" badge.
- Vault creation is confirmed the same way (`VaultCreated` → the agent it was created for).
- A disagreement (different transaction, vault, amount or winner) is flagged as "stream mismatch" in Activity. A timeout refund or deferred payout that the backend did not send shows up as an observed escrow event.
- Settings shows the stream's health: last delivery, events received, how many were confirmed, median block-to-delivery latency.

So the Arc lifecycle has two independent views of the chain, the backend's own RPC reads and QuickNode's push feed, and the UI shows, per step, that they agree.

## Why Streams confirms instead of acting (D-74)

A stream event never moves a challenge's state and never credits or debits the ledger. The HMAC proves a delivery came from QuickNode; it does not prove the chain state, and the backend already reads the escrow before every step. Letting a pushed event move money would add a second, weaker authority. As a cross-check it is useful, visible and safe to get wrong.

## Verified with real deliveries (2026-11-19)

- ~5,400 unfiltered Arc testnet logs, then the filtered escrow lifecycle, were accepted with the documented `x-qn-signature` / `x-qn-timestamp` / `x-qn-nonce` headers and HMAC-SHA256(nonce + timestamp + body). The dashboard "Check connection" request is unsigned and correctly refused (401).
- Payload: `{ data: [log, ...], metadata: { network: "arc-testnet", stream_id } }`, `eth_getLogs`-format logs.
- One full lifecycle (2 `VaultCreated`, `Created`, 2 `Deposited`, `Activated`, `Resolved`) arrived, each about 1 s after its block. That real delivery is the test fixture (`backend/crates/server/tests/fixtures/quicknode/arc_escrow_lifecycle.json`), and the tests check that every event is confirmed against the real challenge row.

## Built

- `POST /api/webhooks/quicknode` (`backend/crates/server/src/quicknode.rs`): HMAC with constant-time compare, 300 s timestamp window, idempotent on (tx hash, log index), reorg flag kept.
- `backend/crates/chain/src/events.rs`: decoder for the escrow, factory and vault events. `backend/crates/server/src/streams.rs`: matching, rechecks each tick, `GET /api/streams`, `stream_events` on challenge detail.
- Web: `components/StreamBadge.tsx`, the escrow table's Streams column, the challenge page card, the Activity badge, the Settings status line.
- Tests: `backend/crates/server/tests/quicknode.rs` (14 integration tests: confirmation from the real delivery, events ahead of the backend, mismatch, reorg, unknown ids, vault releases, signatures, idempotency), unit tests in `streams.rs` and `events.rs`, `web/lib/streams.test.ts`.

## Honest limits

- Per-agent vault logs are filtered out of the live stream today, so vault releases show "not streamed". The backend already confirms them; the owner adds the vault addresses to the filter (`docs/setup/quicknode-streams-filter.md`, addresses from `GET /api/streams`).
- Resolution is backend-administered (the backend's resolver key calls `resolve`). Streams confirms what happened onchain, not that the outcome was right.
- Whether QuickNode's logs carry `blockTimestamp` (Arc's RPC does) is not confirmed yet; without it the latency reads n/a.
