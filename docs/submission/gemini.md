# Gemini Prediction Markets (draft)

Criteria as published: "Projects must be built on Gemini's prediction market APIs." Gemini here is the Gemini exchange, not Google's models.

## How we use it

- Agents discover opportunities from Gemini prediction-market **events and contracts** (public read API, no key). Each challenge is a disagreement about one contract: where its YES price will be at the end time, or how it resolves if Gemini settles it first (D-14, owner to confirm).
- Spot ticker and candles are a fallback signal only, and a challenge built on spot says so.
- Shapes were probed live on 2026-11-15 and are documented in `docs/research/gemini-prediction-markets.md`; the client (`backend/crates/server/src/market/gemini.rs`) models only observed fields. Tests replay trimmed real responses against a local server.

## Honest limits

- We read Gemini data; we do not trade on Gemini or create markets (that needs an account, key and accepted terms).
- Resolution reads the contract price or published outcome from Gemini through our backend, labelled backend-administered. A market data API is not an oracle.
- The scripted offline demo uses a deterministic fake market (`MARKET_DATA=fake`) so the numbers are reproducible; the live client is the default (`MARKET_DATA=gemini`).
- Discovery used to take the contract's last trade price as the claim's threshold; on thin 15-minute contracts that was far from the book (in the live run: last 0.14 while the book was 0.34/0.41). Since 2026-11-18 (D-67) discovery and the market preview use the YES bid/ask **mid** when both sides are quoted and not crossed, else the last trade; resolution is unchanged. This is unit-tested on real response shapes but **not verified live**: Gemini's PM API answered HTTP 500 to every query from about 05:44 UTC until at least 06:32 UTC on 2026-11-18.
- When the PM API is down, discovery falls back to BTC spot and says so in the challenge. In one testnet run Gemini's spot `last` stayed unchanged for 17 minutes and the challenge resolved as a tie on that stale value (correct per the published formula; a mid-price or staleness rule is the owner's call).

## Verified live (2026-11-17/18)

A full challenge ran end to end on live Gemini data with real Arc testnet USDC, driven by the autonomous scheduler: the bull agent picked `GEMI-BTC15M2611180000-UP` ("BTC price today at 7pm EST: Up") from `/v1/prediction-markets/events`, the agents negotiated a claim on its YES price at 00:01:54 UTC, and after the end time the backend read the settled event from `/v1/prediction-markets/events/BTC15M2611180000` and resolved the escrow onchain from Gemini's published outcome. Gemini settled the event at 00:00:08 UTC with `resolutionSide: "no"` (settlement 64,874.75 against the 64,923.72 reference), the backend observed 0.00 at 00:02:02, and the skeptic won 1.25 testnet USDC ([resolve tx](https://explorer.testnet.arc.io/tx/0x602f22fb4840d36f2b827fd6c2fa5b256d172c05593d00e2723892d72c91a513)). Record: `reports/handoffs/2026-11-17-testnet-gemini.md`.

Also seen live: `resolutionSide: "no"` on settled ladder contracts (only `"yes"` had been seen before), and BTC events settle about 6 s after their expiry.
