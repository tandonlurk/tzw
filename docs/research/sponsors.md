# Sponsor research (verified 2026-11-14)

Legend: VERIFIED = read in official docs (URL given) or probed live. UNVERIFIED = not confirmed; what was tried is stated.
Note: Arc docs moved from docs.arc.network to docs.arc.io (301). Several Gemini/QuickNode doc pages render client-side, so some parameter tables could not be read.

## 1. Arc (Circle L1)

| Item | Value | Source |
|---|---|---|
| Testnet chain ID | 5042002 (probed `eth_chainId` on RPC, returned live block 0x4cef52) | https://docs.arc.io/arc/references/connect-to-arc |
| Mainnet | Live. Chain ID 5042, `https://rpc.mainnet.arc.io`, explorer `https://explorer.arc.io`. DO NOT USE (real funds need owner approval) | same |
| Testnet RPC (primary, anonymous) | `https://rpc.testnet.arc.io`, WS `wss://rpc.testnet.arc.io` (eth_subscribe WS only) | https://docs.arc.io/arc/references/rpc-endpoints.md |
| Other testnet RPCs | Blockdaemon `https://rpc.blockdaemon.testnet.arc.io`; dRPC `https://rpc.drpc.testnet.arc.io`; QuickNode `https://rpc.quicknode.testnet.arc.io` (public aliases) | same |
| Explorer | `https://explorer.testnet.arc.io` (Blockscout; API at `/api/`) | connect-to-arc; deploy-on-arc.md |
| Faucet | `https://faucet.circle.com` (select Arc Testnet). Limits NOT documented in Arc docs: UNVERIFIED (check faucet page, needs interactive UI) | https://docs.arc.io/arc/tutorials/deploy-on-arc.md |
| Gas token | USDC (native). Min `maxFeePerGas` 20 gwei (lower = silently dropped, no receipt); max base fee 20,000 gwei; tip 0 ok; ~0.001 USD per ERC-20 transfer target | https://docs.arc.io/arc/references/gas-and-fees.md |
| Decimals | Native USDC = 18 decimals; ERC-20 interface = 6. Same balance. `native / 1e12 = ERC-20 units`. Use ERC-20 `decimals()`; never mix `msg.value` with `balanceOf()` | https://docs.arc.io/arc/references/contract-addresses.md |
| USDC (testnet AND mainnet) | `0x3600000000000000000000000000000000000000` | same |
| EURC testnet | `0x89B50855Aa3bE2F677cD6303Cec089B5F319D72a` | same |
| Multicall3 | `0xcA11bde05977b3631167028862bE2a173976CA11` | same |
| Permit2 | `0x000000000022D473030F116dDEE9F6B43aC78BA3` | same |
| CREATE2 factory | `0x4e59b44847b379578588920cA78FbF26c0B4956C` | same |
| ERC-8004 testnet (IdentityRegistry / Reputation / Validation) | `0x8004A818BFB912233c491871b3d84c89A494BD9e` / `0x8004B663056A597Dffe9eCcC1965A193B7388713` / `0x8004Cb1BF31DAf7788923b405b754f57acEB4272` (possible agent-identity fit) | same |
| ERC-8183 AgenticCommerce ref impl (testnet) | `0x0747EEf0706327138c69792bF28Cd525089e4583` (job/escrow reference, could study) | same |
| Finality | Deterministic, <1s, no reorgs. Block time ~0.5s (gas page: "0.5-second block time"; consensus page gives no block time) | https://docs.arc.io/arc/concepts/deterministic-finality.md |
| viem | Chain exported as `arcTestnet` | connect-to-arc |
| Alloy | No Arc preset known: UNVERIFIED. Use chain ID + RPC URL manually | - |

### EVM / Solidity compatibility
Baseline Osaka (+EIP-7708). Source: https://docs.arc.io/arc/references/evm-differences.md
- `PREVRANDAO` = 0 (no onchain randomness). `BLOBHASH` = 0, `BLOBBASEFEE` = 1, blob txs rejected. EIP-4788 beacon roots contract absent. Withdrawals empty.
- `block.timestamp` non-decreasing, 1s granularity: multiple blocks share a timestamp. Use block number for ordering; do not rely on strict timestamp progression for challenge deadlines at sub-second scale.
- Blocklist: value transfers to/from blocklisted addresses revert (USDC compliance). Escrow must handle a reverting payout (use pull-payment / per-party claim, not push loops).
- SELFDESTRUCT: EIP-6780 plus native-value rules; avoid.
- Native transfer to `0x0` reverts.
- Solc version / `evm_version`: NOT stated in docs (UNVERIFIED). Suggest solc 0.8.28-ish with `evm_version = "prague"` or default, and test with `arc-forge`; docs say use same settings for verify as deploy.
- Standard `anvil` does not reproduce Arc; use `arc-anvil --network arc` (can fork testnet).

### Foundry deploy + verify (exact, from https://docs.arc.io/arc/tutorials/deploy-on-arc.md and install-arc-foundry.md)
Arc ships a Foundry fork: download binaries from the Arc Foundry GitHub releases page, rename `forge/cast/anvil` to `arc-forge/arc-cast/arc-anvil`. Release URL not given in the pages fetched: UNVERIFIED; find via the install page. Intel Macs must build from source. Whether stock `forge` works is not stated.
```
arc-forge test --network arc
arc-forge create src/X.sol:X --rpc-url https://rpc.testnet.arc.io --private-key $PRIVATE_KEY --broadcast
arc-forge verify-contract $ADDR src/X.sol:X --chain-id 5042002 --verifier blockscout --verifier-url https://explorer.testnet.arc.io/api/ [--constructor-args $(arc-cast abi-encode "constructor(...)" ...)]
```
Gotchas: fund the deployer from the faucet first (USDC is gas); set gas price >= 20 gwei in scripts; `eth_getLogs` max 10,000 blocks per request (error -32012), so page in chunks of 9,999 (https://docs.arc.io/arc/references/rpc-endpoints.md); no documented rate limits on public RPC.

## 2. QuickNode Streams

| Item | Finding | Source |
|---|---|---|
| Supports Arc testnet? | YES. Arc Mainnet and Testnet both listed; datasets: Block, Block with Receipts, Transactions, Logs, Receipts, Traces. | https://www.quicknode.com/docs/streams/supported-chains ; https://www.quicknode.com/docs/arc/api-overview (testnet chain ID 5042002, WS yes, archive yes) |
| Destinations | Webhook, S3, PostgreSQL, Azure Blob, Kafka | https://www.quicknode.com/docs/streams/destinations |
| Datasets (EVM) | `Logs`, `Block with Receipts` (also block, receipt, trace) | https://www.quicknode.com/docs/streams/data-sources |
| Filtering | Server-side JS/Go `main(stream)` function; example decodes receipts with `decodeEVMReceipts(receipts,[abi])` and filters by event name; return `null` to drop a batch. Filtering by contract address in filter code is possible by inspecting log `address`; exact logs payload field schema NOT documented in pages read (UNVERIFIED): use `console.log` in the Filter Editor to inspect | https://www.quicknode.com/docs/streams/filters |
| Payload | `{ data: [...dataset in chain-native format...], metadata: {...} }`. **VERIFIED 2026-11-18** for Logs: `data` is an array of per-block arrays of logs with `address, topics, data, blockNumber, transactionHash, transactionIndex, blockHash, logIndex, removed` (https://www.quicknode.com/docs/streams/data-sources/ethereum-evm.md). Metadata (also sent as HTTP headers): `network, dataset, batch_start_range, batch_end_range, blocks_reorged, reorgs, stream_id, stream_name, keep_distance_from_tip,...` | data-sources page |
| Webhook | POST JSON (optionally gzip); custom headers; retry default every 1s, pauses after 3 failed retries, timeout 30s; must return 2xx. **VERIFIED 2026-11-18**: headers `X-QN-Nonce`, `X-QN-Signature`, `X-QN-Timestamp`; signature = hex HMAC-SHA256(stream security token, nonce + timestamp + decompressed body), no separators. No stream-id/batch header documented | https://www.quicknode.com/docs/streams/destinations/webhooks ; https://www.quicknode.com/guides/quicknode-products/streams/validating-incoming-streams-webhook-messages |
| Real deliveries (Arc testnet) | **VERIFIED 2026-11-19** with stream `tzw-arc-testnet-escrow` (Logs, webhook through the owner's cloudflared tunnel, no compression): headers `x-qn-signature` / `x-qn-timestamp` / `x-qn-nonce` and HMAC-SHA256(security token, nonce + timestamp + body) accepted as implemented (the server stores a log only after the HMAC and 300 s window pass; ~5,400 unfiltered logs from blocks 73111970-73112299, then the filtered lifecycle). `metadata.network = "arc-testnet"`, `metadata.stream_id` = the stream's UUID. Every log carried `address, topics, data, blockNumber, blockHash, transactionHash, logIndex, removed`; with the owner's JS filter `data` is one flat array of logs. The dashboard "Check connection" request is **unsigned** (401 `missing_signature` from us); real deliveries are signed. Latency: ~1 s from the block's timestamp to the row stored, for all seven escrow/factory events of one lifecycle (block 73112749 at 01:52:01 UTC stored 01:52:02.3, ..., `Resolved` in block 73113135 at 01:55:17 stored 01:55:18.2). Arc's RPC includes `blockTimestamp` on logs; whether QuickNode's Logs payload carries it is not yet confirmed (the server did not store unknown fields before D-74; it does now, so the next delivery answers it). Fixture: `backend/crates/server/tests/fixtures/quicknode/arc_escrow_lifecycle.json` | aen_dev `chain_events` rows (read-only), Arc RPC `eth_getBlockByNumber` |
| Delivery | Ordered, batch-by-batch; FAQ claims "exactly-once delivery in finality order", but docs also say that after a restart batches may be re-sent, so dedupe on `(tx_hash, log_index)` anyway | https://www.quicknode.com/docs/streams/faq ; destinations page |
| Reorgs | `blocks_reorged`/`reorgs` metadata for re-delivery. Arc has deterministic finality, so reorgs should not occur | metadata table; Arc finality page |
| Limits / cost | Billed in API credits by network, dataset, blocks processed. No permanent free plan; 1-month free trial: 10M credits, 15 RPS, Streams 1 GB/month (support article snippet; confirm in dashboard). Paid Build plan $49/mo. | https://www.quicknode.com/pricing ; https://support.quicknode.com/articles/5387074982-1-month-free-trial (via search snippet) |
| Webhook needs public URL | Backend must be reachable (use a tunnel such as ngrok/cloudflared in dev, or a hosted backend) | inference |
| Fallback | QuickNode Arc RPC (`wss://rpc.quicknode.testnet.arc.io` public alias, or own endpoint) with `eth_subscribe logs` / `eth_getLogs`. Per UBC bounty, "Streams must be a core data source" so fallback alone likely does NOT qualify | https://www.universityblockchain.org/programs/hackathon |

## 3. Gemini crypto exchange

Base URL `https://api.gemini.com`. Sandbox REST `https://api.sandbox.gemini.com`, WS `wss://ws.sandbox.gemini.com`; sandbox market data is bot-simulated, NOT real prices (https://developer.gemini.com/get-started/sandbox.md).

| Need | Endpoint | Notes |
|---|---|---|
| Ticker | `GET /v1/pubticker/{symbol}` | PROBED live, no auth: `{"bid","ask","last","volume":{...}}`. Symbol e.g. `btcusd` (case-insensitive in probe) (https://developer.gemini.com/trading/rest-api/market-data/get-ticker) |
| Candles | `GET /v2/candles/{symbol}/{time_frame}` | PROBED live, `1hr` works; rows `[ts_ms, open, high, low, close, volume]`, newest first. Full timeframe list not in fetched doc: UNVERIFIED (known-from-use: 1m,5m,15m,30m,1hr,6hr,1day) |
| Symbols | `GET /v1/symbols` and `/v1/symbols/details/{symbol}` | docs: https://developer.gemini.com/trading/rest-api/market-data/list-symbols.md (not fetched in detail) |
| Spot WS | `wss://ws.gemini.com`, subscribe `{"id":"1","method":"SUBSCRIBE","params":["btcusd@bookTicker"]}`; streams `@bookTicker, @trade, @depth5/10/20, indexPrice`; no auth for public streams | https://developer.gemini.com/trading/websocket/streams.md. Spot symbol casing in WS UNVERIFIED |
| Rate limits | Public 120/min (recommended 1/s); private 600/min (5/s) | https://developer.gemini.com/rate-limit.md |

Prediction markets: YES, real, accessible. Public listing PROBED live without auth: `GET https://api.gemini.com/v1/prediction-markets/events?limit=1` returned e.g. event "BTC price on November 14", ticker `BTC2611142200`, category Crypto, `contracts[]` with labels like "$64,500 or above", resolution text "outcome verified against the LS-LRRR-XBTUSD index" (https://developer.gemini.com/rest-api/prediction-markets/events/list-events.md). Other endpoints: list-upcoming/recently-settled/newly-listed events, categories, get-event, get-event-strike (`/rest-api/prediction-markets/events/*.md`). PM WS symbols look like `GEMI-BTC05M2606011000-UP` (5-minute BTC up/down contracts). Trading requires API key plus accepting PM terms in the Gemini UI (search-result summary, UNVERIFIED in primary doc). Query params/response schema pages did not render: UNVERIFIED. Gemini cannot create arbitrary markets; only use listed events/contracts.

Auth for private endpoints (https://developer.gemini.com/authentication/api-key.md): headers `X-GEMINI-APIKEY`, `X-GEMINI-PAYLOAD` (base64 JSON incl. `request`, `nonce`), `X-GEMINI-SIGNATURE` (hex HMAC-SHA384 of payload with secret), `Content-Type: text/plain`, `Content-Length: 0`; empty body. Nonce: unix seconds +-30s (time-based keys) or strictly increasing. Sandbox account: https://exchange.sandbox.gemini.com.

## 4. xAI / Grok

| Item | Value | Source |
|---|---|---|
| Base URL | `https://api.x.ai/v1` | https://docs.x.ai/docs/guides/structured-outputs |
| Auth | `Authorization: Bearer $XAI_API_KEY` | https://docs.x.ai/docs/guides/function-calling |
| Endpoints | `/v1/responses` documented in examples; OpenAI SDK compat with `base_url=https://api.x.ai/v1` (chat.completions.parse and responses.parse shown). Raw `/v1/chat/completions` path not explicitly documented in fetched pages: UNVERIFIED. 2026-11-18 re-check: `llms.txt` lists it as "Chat Completions (Legacy)" and the API reference calls `/v1/responses` the primary interface; the structured-outputs guide still shows `client.chat.completions.create` with `response_format`. Our client uses chat completions; if a live call fails, switch to `/v1/responses` | same; https://docs.x.ai/llms.txt ; https://docs.x.ai/docs/api-reference |
| Models (text) | grok-4.7 (recommended), grok-4.6, grok-4.5, grok-4.3, grok-4.20-0309-reasoning, grok-4.20-0309-non-reasoning, grok-4.20-multi-agent-0309, grok-build-0.1. Aliases: `name`, `name-latest`, `name-<date>` | https://docs.x.ai/docs/models |
| Pricing per 1M tokens (<200k prompt) | grok-4.7/4.6: $2 in / $0.50 cached / $6 out; grok-4.3 & 4.20: $1.25 / $0.20 / $2.50 | same |
| Structured output | `response_format: {type:"json_schema", ...}` (also `json_object`); supports string, number, integer, boolean, null, enum, const, array, object, anyOf/oneOf, single-subschema allOf, `$ref/$defs`; `additionalProperties` defaults false; 400 for empty enum/anyOf, boolean subschemas, array-form `items`; best-effort: `not`, if/then/else; limits: string length 2048, items 256, properties 64. Type arrays (`["string","null"]`) not listed, so our schemas use `anyOf` with `{type: null}` (2026-11-18). Validate in Rust regardless | structured-outputs page |
| Function calling | Yes. `tool_choice` auto/required/none/named; parallel calls default on; tool args conform to schema (strict). Can combine with structured output. Server-side tools (web search, X search) on Grok 4 family | function-calling page |
| Rate limits | Per-team RPS and TPM by spend tier: T0 $0, T1 $50, T2 $250, T3 $1000...; 429 on exceed; numeric limits per model not in page | https://docs.x.ai/docs/key-information/consumption-and-rate-limits |
| Free credits | Not mentioned in docs: UNVERIFIED. Check console / hackathon perks. UBC page lists X Premium perk, not API credits | - |

## 5. ElevenLabs (lowest priority)

- Base `https://api.elevenlabs.io`, paths under `/v1/`, auth header `xi-api-key` (https://elevenlabs.io/docs/api-reference/authentication.md).
- TTS: SDK `textToSpeech.convert(voice_id, text, modelId)`; models Eleven v4 (10k chars), v4 Turbo / Flash v2.5 (low latency, 40k chars on Flash v2.5). HTTP path `POST /v1/text-to-speech/{voice_id}` is from memory, UNVERIFIED in the pages fetched (https://elevenlabs.io/docs/overview/models.md).
- Conversational agents ("ElevenAgents"): create via `conversationalAi.agents.create({conversationConfig:{tts,agent:{prompt,first_message}}})`; test in dashboard; embed `<elevenlabs-convai agent-id>` widget (https://elevenlabs.io/docs/eleven-agents/quickstart.md). Signed-URL/WS details UNVERIFIED (see https://elevenlabs.io/docs/eleven-agents/api-reference/llms.txt).
- Free plan: 10,000 credits/month (~10 min audio), non-commercial, attribution; concurrency Free: 2 (Multilingual v2) / 4 (Flash) (https://elevenlabs.io/pricing via search snippet; models page for concurrency). UBC gives Creator for 1 month to complete submissions.

## 6. Bounties: the event is identified

Event: **University Blockchain Conference (UBC) 2026 Hackathon** (UT Austin finals; online build). It is NOT at UBC Vancouver. Source: https://www.universityblockchain.org/programs/hackathon (Devpost https://ubc-hackathon-2026.devpost.com/ returned 403; unread).

**DEADLINE CONFLICT:** page says submission deadline **Nov 20, 12 PM CST** on Devpost; finals Nov 21 at UT Austin (up to 13 teams); kickoff Nov 12; sponsor office hours Nov 16-19. AGENTS.md assumes Nov 19. Plan for Nov 19 internally but note Nov 20 noon CST is the hard limit.

| Award | Prize | Criteria as published |
|---|---|---|
| Main track (pick exactly one): Agentic Payments & AI / DeFi & Onchain Apps / Consumer Apps (needs working frontend) | $2,000 per track | "AI agents with wallets, budgets, guardrails" fits track 1 |
| Best use of Arc (chain award, chosen by Circle) | $1,000 per track | Arc testnet is fine |
| Official UBC winner | $2,500 / $1,000 / $500; 1st also 3 mo ElevenLabs Pro per member | - |
| Gemini Prediction Markets bounty | $4,000 total ($2,500/$1,000/$500) | "Projects must be built on Gemini's prediction market APIs". Spot ticker/candles alone likely insufficient; use PM events/contracts |
| QuickNode Streams bounty | $500 | "QuickNode Streams must be a core data source" |
| ElevenLabs bounty | 3 months Scale per member | "Voice or audio must be central to the project" |
| Grok Bot bounty | mechanical keyboard + swag per member | "Grok Bot must do real work in the project"; Devpost snippet says Consumer track judged by Cursor/xAI + UBC rep (secondary source) |

Each bounty judged by sponsor + UBC rep; can enter many bounties. Submit via Devpost: one main track, optional one chain award, any bounties. Detailed deliverable list (video, repo) not on page: UNVERIFIED, check the full guide/FAQ. Note "Grok Bot" implies a bot product; exact meaning UNVERIFIED, ask in sponsor office hours.

## 7. Risks, unresolved questions, recommendations

- Deadline mismatch (Nov 20 noon CST vs Nov 19 in AGENTS.md): confirm on Devpost; keep Nov 19 internal target.
- Arc Foundry fork required? Binary download URL and solc/evm_version unconfirmed: first task for contracts agent is installing arc-forge and deploying a trivial contract.
- Faucet limits unknown; a single deployer key plus 4-5 agent wallets need funding early. Fund on day 1.
- 20 gwei minimum fee: Alloy default gas estimation may underprice; set explicit `max_fee_per_gas` >= 20 gwei and check receipts.
- Blocklist reverts and 6/18-decimal split: use ERC-20 USDC interface (6 decimals) only in contracts and DB; never `msg.value`.
- Gemini PM: response schemas unread; resolution authority for our challenges (BTC > 5% in 24h) is OUR backend/oracle, not Gemini. Gemini PM contracts are Gemini-settled and can be mirrored/referenced, not created. Gemini PM trading needs real account + terms: Gemini sandbox PM support UNVERIFIED.
- QuickNode Streams needs a public webhook URL and a trial/paid plan: sign up now (trial 1 GB may be tight, but Arc testnet volume is small; filter server-side).
- Stream payload field schema for Logs unseen: capture a real payload early.
- xAI free credits unknown; budget ~$ small; use grok-4.3 for cheap runs, structured output with Rust-side validation.
- Grok Bot meaning unclear.

One-line recommendations:
- Arc: go, testnet only, USDC `0x3600...0000`, chain 5042002, deploy with arc-forge, verify via Blockscout.
- QuickNode Streams: go, webhook stream on the escrow contract's events (filter in JS), with an `eth_getLogs` poller as a dev fallback.
- Gemini: go, use public PM events plus spot candles as opportunity source; keep resolution in own oracle (labelled manually adjudicated / index-based).
- xAI: go, grok-4.x via `/v1` OpenAI-compatible with json_schema outputs; validate everything server-side.
- ElevenLabs: last, TTS for co-captain summaries via REST with `xi-api-key`; only if time remains.
