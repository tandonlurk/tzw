# Gemini exchange: public market data and prediction markets (live spike)

"Gemini" here is the **Gemini crypto exchange** (`api.gemini.com`), never Google's AI models.

Probed live with `curl`, no API key, on **2026-11-15** (about 20:45 UTC). Everything under "Observed" was seen in a real response. Anything not seen is listed under "Not verified". The client in `backend/crates/server/src/market/gemini.rs` models only observed fields; its tests replay trimmed copies of these bodies against a local server and never touch the network.

## Verdict

- The prediction-market **read** API is public, unauthenticated and usable: events, their contracts, live contract prices, and the published outcome of settled contracts.
- So challenges are built on **prediction-market contracts** (decision D-14): two agents disagree about where one contract's YES price will be at `end_ts` (or how the contract resolves, if it settles first). Spot price is the fallback when no suitable contract exists, and the challenge then says so.
- We do **not** trade on Gemini, create markets, or need an API key. Trading needs a key and accepted terms (not verified, not needed).
- A market data API is not an oracle. Resolution is read by our backend and labelled backend-administered.

## Observed: spot

| Request | Status | Body |
|---|---|---|
| `GET /v1/pubticker/btcusd` | 200 | `{"bid":"65016.06000","ask":"65016.07000","last":"65029.83000","volume":{"BTC":"306.59686938","USD":"19937942.2943136054","timestamp":1794775740000}}` |
| `GET /v1/pubticker/nopeusd` | 404 | plain text, not JSON: `'nopeusd' does not have available data yet` |
| `GET /v2/candles/btcusd/1m` | 200 | JSON array of `[ts_ms, open, high, low, close, volume]`, **numbers not strings**, newest first, about 1440 rows (24 h) |
| `GET /v2/ticker/btcusd` | 200 | `{"symbol":"BTCUSD","open":"65397.46","high":"66081.41","low":"61998","close":"65033.76","changes":[24 hourly closes as strings],"bid":"...","ask":"..."}` (not used by the client) |

Prices in `pubticker` are decimal strings. We parse them to integer micro-USD without floats (extra digits truncated). Candle prices are JSON numbers; we convert through their decimal text. Candles are only a discovery signal (spot move over a window), never a resolution input.

## Observed: prediction markets

| Request | Status | Notes |
|---|---|---|
| `GET /v1/prediction-markets/events?limit=2` | 200 | `{"data":[Event],"pagination":{"limit":2,"offset":0,"total":1002}}` |
| `GET /v1/prediction-markets/events?limit=3&category=Crypto&status=active` | 200 | filters work (`total` 98); returned `BTC05M2611152050`, `ETH05M...`, `SOL05M...` |
| `GET /v1/prediction-markets/events?limit=3&search=BTC` | 200 | search works; returned `BTC2611152200`, `BTC2611152100`, `BTC2611202200` |
| `GET /v1/prediction-markets/events?limit=100&search=BTC&category=Crypto&status=active` | 200 | `limit=100` and `limit=500` accepted; `total` 24. **Sorted soonest-expiring first**: the first 20 were almost all 5 and 15 minute events (`BTC05M...`, `BTC15M...`), most with no trade yet. The longer ladders (`BTC2611202200`, `BTC2611302200`) came last |
| `GET ...events?limit=20&offset=20&...` | 200 | `offset` works (returned the remaining 4) |
| `GET /v1/prediction-markets/events/{ticker}` e.g. `HYPE2611152200` | 200 | one Event object (not wrapped in `data`) |
| `GET /v1/prediction-markets/events/461869` (numeric id) | 404 | `{"error": "Event not found"}`: lookup is by **ticker**, not id |
| `GET /v1/prediction-markets/events/recently-settled?limit=1` | 200 | same list shape, settled events |
| `GET /v1/prediction-markets/events/newly-listed?limit=1` | 200 | same list shape |
| `GET /v1/prediction-markets/events/upcoming?limit=1` | 200 | `{"data":[],"pagination":{...,"total":0}}` at the time |
| `GET /v1/prediction-markets/categories` | 200 | `{"categories":["Live","Sports","Pro Football","Crypto","Commodities","Economics","Economy","Fun/Culture","Media","Politics","Tech","Business","Weather","Culture","Other","Finance"]}` |

### Event (fields seen)

`id` (string), `title`, `slug`, `type` (`"categorical"` or `"binary"`), `category`, `ticker`, `status` (`"active"`, `"settled"`), `createdAt`, `effectiveDate`, `expiryDate`, `startTime`, `isLive`, `participants` (array), `description`, `imageUrl`, `socialImageUrl`, `featuredImageUrl` (some), `series` (e.g. `HYPE1H`, `XRP15M`), `volume`, `volume24h`, `volumeDelta24hPct` (strings), `tags`, `eventTags` (`[{id,name,slug}]`), `subcategory` (`{id,slug,name,path}`), `template` (`prices-ladder`, `crypto-up-down`), `termsLink`, `source` (e.g. `LS-LRRR-HYPE22USD`), `sourceDetails` (`{agency:"lukka", index}`), `contracts` (array). Settled events add `resolvedAt` and `settlement: {value}`.

### Contract (fields seen)

`id`, `label` (e.g. `"$59 or above"`, `"Up"`), `description` (rich-text object), `ticker` (e.g. `HI59`, `UP`), `instrumentSymbol` (e.g. `GEMI-HYPE2611152200-HI59`), `status`, `marketState` (`"open"`, `"closed"`), `createdAt`, `effectiveDate`, `expiryDate`, `abbreviatedName`, `color`, `sortOrder`, `isCombo`, `source`, `termsAndConditionsUrl`, `quantityIncrement`, `quantityMinimum`, `priceIncrement` (`"0.01"`), `priceMinimum`, `quoteAssetPrecision`, and:

- `prices` (an open contract that has **not traded yet has no `lastTradePrice`**, seen on most upcoming 5 minute events and on the top strike of a ladder; we treat that as missing data): open contract `{"buy":{"yes":"0.99","no":"0.05"},"sell":{"yes":"0.95","no":"0.01"},"bestBid":"0.95","lastTradePrice":"0.99"}`; `bestAsk` appears on some contracts and not others. **Settled contract: `{"buy":{},"sell":{}}`**, so there is no last price after settlement.
- `strike`: `{"type":"above","value":"59"}` on ladder contracts; `{"type":"reference","value":"0.95996973755","availableAt":"..."}` on up/down contracts.
- Settled only: `resolutionSide` (`"yes"` seen), `resolvedAt`, `settlementValue` (e.g. `"0.9625122158833334"`).

Contract rules text seen: "If the price of HYPE is $59 or above on November 15, 2026 at 5pm ET, this market will resolve to Yes. Otherwise, it will resolve to No. ... Outcome verified against the LS-LRRR-HYPE22USD index." The strike convention is "or above" (>=), which is why our "above" side wins at exactly the threshold.

## How the backend uses it

| Step | Call | Field |
|---|---|---|
| Spot move (trigger, discovery signal) | `/v2/candles/{symbol}/1m` (`1hr` for windows over 24 h) | latest close vs the close one window earlier |
| Find a contract | `/v1/prediction-markets/events?search={BASE}&category=Crypto&status=active&limit=100` | events whose `ticker` starts with the base asset, `expiryDate` after the funding deadline; the open contract whose `lastTradePrice` is closest to 0.50 and within 0.05 to 0.95 |
| Resolve (formula `pm_yes_price_bps_v1`) | `/v1/prediction-markets/events/{event_ticker}` | the contract's `resolutionSide` (yes = 10000, no = 0) if settled, else `prices.lastTradePrice` × 10000; first successful read at or after `end_ts` |
| Fallback (formula `spot_last_price_micros_v1`) | `/v1/pubticker/{symbol}` | `last` in micro-USD |

Missing price, missing contract, an unknown `resolutionSide`, a non-2xx answer or a timeout are all errors. They are retried until the grace period ends, then the challenge is voided and refunded. Nothing defaults to a number.

Client behaviour: 5 s timeout per request, 3 attempts, backoff 250 ms doubling, retry only on timeouts, transport errors, 429 and 5xx. Rate limit per `sponsors.md`: 120 requests/min public; a tick makes at most 3 calls per proposing agent and 1 per resolving challenge.

## Not verified (do not rely on)

- `resolutionSide: "no"`: only `"yes"` was seen. We accept `"no"` by symmetry and treat anything else as missing data.
- Other `status` values (e.g. suspended, cancelled) and how a cancelled contract is reported. An unknown state without a price or resolution ends in a void, which is the safe outcome.
- `get-event-strike` endpoint, the WebSocket streams, and all authenticated endpoints.
- Candle timeframes other than `1m` and `1hr`.
- That `lastTradePrice` right after `end_ts` reflects the price at `end_ts`: it is the latest trade at read time. Stated as part of the formula ("first successful read at or after `end_ts`").
