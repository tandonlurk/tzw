# Handoff 2026-11-18 (overnight item 2) — testnet gaps + live Arc run

Branch `overnight-1118`. Code: 4dd6a8f. Decision D-67.

## What changed
- Vault `release` txs stored per side (`proposer_release_tx`, `counterparty_release_tx`, migration 0011), shown under `escrow` on the interaction, one `vault_released` activity row each (with `tx_hash`, `explorer_url`). Nothing staked → no tx, no row.
- `GET /stats/portfolio?env=simulated|arc_testnet` (default simulated, else 400). Envs are never summed. Dashboard shows a second panel for testnet when an agent is on `arc_testnet`.
- Discovery, approach filter and `/markets/preview` price a prediction contract at the YES bid/ask mid (rounded down) when both sides are quoted and not crossed, else last trade. Resolution formula unchanged.
- Tests: `cargo test --workspace` 391 passed, 1 ignored (chain tests ran on anvil); clippy clean; web 112 tests, build OK.

## Live Arc testnet run (2026-11-18 05:43–06:06 UTC)
Fresh db `aen_arc_overnight`, `MARKET_DATA=gemini`, `FUNDING_WINDOW_SECS=180`, manual `/dev/tick`, `AEN_DEV_CHAIN_OWNER=1`. Vaults 2.50 each; bull config edited as in `docs/demo.md` (trigger 1 bps, conviction 5000, horizon 851 s); bull paused once active.

- **Gemini prediction-markets API returned HTTP 500 on every query for the whole run** (polled every minute 05:44–06:02 UTC, any params). Discovery fell back to BTC spot as designed (`fallback_reason: prediction markets unavailable: ... HTTP 500`). So the bid/ask-mid change is **not verified live**; it is covered by unit tests on the real response shapes.
- Claim: bull "BTCUSD spot below 64321.41 at 06:01 UTC", 2.50 offered, skeptic countered 1.25, approval (above 1.00 limit) approved, escrow create + 2 stakes → active.
- Resolve at 06:01:50: observed 64321.41 = threshold, so "at or above" (skeptic) won. **Finding:** Gemini spot `last` did not change for 17+ min (book 64,242.83 / 65,249.98 at 06:02), so the spot formula resolved on a stale last trade. Correct per `spot_last_price_micros_v1`, but a tie on a stale price is a poor demo. Not fixed (resolution change; owner call): options are a spot mid formula (`spot_mid_price_micros_v2`) or voiding when `last` is older than N minutes.
- After settle + release: bull 1.25 onchain = 1.25 ledger, skeptic 3.75 = 3.75, exposure 0, drift 0. `/stats/portfolio?env=arc_testnet`: allocated 5.00, committed 0, settled 1, 2 P&L points, realized −1.25 / +1.25 (net 0, one owner). Simulated portfolio empty.
- Withdrew both vaults to the test owner and synced: 0 = 0, drift 0. No server errors besides the Gemini 500 warnings; no RPC rate limits.

| Step | Tx (https://explorer.testnet.arc.io/tx/<hash>) |
|---|---|
| Bull vault create + link | 0xd5cc83988891c97a1a12d4d2a23f5699a2a8a176c94ed85406fe6fcc2e34e567 |
| Bull vault fund 2.50 | 0x000ab00761df7bbc6c4b3e8ad571906f204526f47a114de23b778111ad310d8b |
| Skeptic vault create + link | 0x4d630b217c085124d24820375789d2bfd32fea9816d1468d0071fa78ab9877b1 |
| Skeptic vault fund 2.50 | 0xd02a67df3b78d6277d3243c16a15e99bbb62d1ca89a7ba3ba5fe7a518b163a2b |
| Escrow create | 0x19643bae86291031a856b12b7ff36433c8960615fc4f464aa2edc6d9348d18a5 |
| Bull stake 1.25 | 0x6a8413f7bf63a7d75ef02a6abd8a307305d2a9255e64ac04da2cce16ac0f7cd1 |
| Skeptic stake 1.25 | 0x2b4f880d69c5946edca1a2adbd829ef1040fc3a393bfc0026f47d31bd1e2a557 |
| Resolve → skeptic | 0x8f4f7c22108b0c53e55c999c09ece8b7f5fb809dd49872dee72b0b97b8d61ddf |
| Bull vault release (now recorded) | 0xb83df8d5e98bd7c70b462160a428b5fb7cc9e42bcc3c526f07f81fa223ea66e7 |
| Skeptic vault release (now recorded) | 0xdba6bc0cfb08842a5dceb593dfee146de51e0db66ef7591e8c9ee256a87f080d |
| Bull withdraw 1.25 | 0x627ca10015d7fdd3d2c6c6fb5b975f6af6d09ba708d73d874fdd839ca511f089 |
| Skeptic withdraw 3.75 | 0xe1194f098339bbd2f080d9338e6a8fb1e143bface3b76941654b36c5590016dc |

Vaults: bull `0xFDc7f581579223bCBBFe5c22b44DCC5Da9EB3fe6`, skeptic `0x453150E5bF9B1515744c0aed61ec8E66C9Aa4EA3`. Interaction `66af9e26-43f3-40a5-911c-39954d258f30`.

Gas (testnet USDC): resolver idx 0 −0.006141, operator idx 1 −0.012815, test owner idx 2 −0.068752 (2 vault creates, fund, withdraw). Total ~0.088; the 5.00 came back. Owner idx 2 now 13.727142.

## Open
- Re-verify the bid/ask mid live once Gemini prediction markets answer again (read-only: `POST /markets/preview` and compare `yes_price_bps` with the event's `bestBid`/`bestAsk`).
- Stale spot `last` finding above (owner decision).
