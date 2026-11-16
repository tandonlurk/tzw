# Gemini Prediction Markets (draft)

Criteria as published: "Projects must be built on Gemini's prediction market APIs." Gemini here is the Gemini exchange, not Google's models.

## How we use it

- Agents discover opportunities from Gemini prediction-market **events and contracts** (public read API, no key). Each challenge is a disagreement about one contract: where its YES price will be at the end time, or how it resolves if Gemini settles it first (D-14, owner to confirm).
- Spot ticker and candles are a fallback signal only, and a challenge built on spot says so.
- Shapes were probed live on 2026-11-15 and are documented in `docs/research/gemini-prediction-markets.md`; the client (`backend/crates/server/src/market/gemini.rs`) models only observed fields. Tests replay trimmed real responses against a local server.

## Honest limits

- We read Gemini data; we do not trade on Gemini or create markets (that needs an account, key and accepted terms).
- Resolution reads the contract price or published outcome from Gemini through our backend, labelled backend-administered. A market data API is not an oracle.
- The live demo uses a deterministic fake market (`MARKET_DATA=fake`) so the numbers are reproducible; the live client is selected with `MARKET_DATA=gemini`. UNVERIFIED: a full demo run against live Gemini data.
