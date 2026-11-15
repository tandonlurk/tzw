# Threat model (M1)

Status: written against M1 code (`aen-core` ledger and `authorize()`, Axum API, `ChallengeEscrow`). Nothing here implements keys, signing, webhooks, LLM calls or custom code yet. Those sections describe PROPOSED designs (D-3, D-8, D-11, D-16) and the controls they must have before they are built. Findings are in `reports/security-m1.md`.

## Assets
| Asset | Where | Notes |
|---|---|---|
| Ledger (simulated, later `arc_testnet`) | Postgres `ledger_entries` | Append-only by trigger; balances derived, never stored |
| Agent policy and versions | `agent_versions` | Immutable rows; policy is the financial control |
| Escrowed stakes | `ChallengeEscrow` | Testnet USDC only |
| Resolver key, per-agent keys (proposed) | env seed (D-3) | Not implemented. Custody needs owner approval |
| API keys (xAI, Gemini, QuickNode, ElevenLabs) | env vars | Not yet used |

## Actors
Owner (single demo user, no auth, D-9); agent logic (blocks, LLM, custom code), all untrusted: they only produce `Intent`s; a hostile web page in the owner's browser; a network attacker reaching a public webhook URL; counterparty agent owners; the escrow resolver (backend key); the token issuer (Circle, upgradeable USDC with blocklist).

## Trust boundaries and threats

### 1. Authorization (agent output to money movement)
- Control: `authorize()` is pure; every staking intent is checked for pause, stop-loss, type, state legality, non-zero, per-interaction max, aggregate exposure (overflow-safe), available balance, approval threshold. Ledger rejects any set that would make a non-external account negative, in the DB path under advisory locks.
- Tested (`adversarial_authorize.rs`, `adversarial_ledger.rs`, `adversarial_api.rs`): boundaries, overflow, zero, malformed money strings, 200k random decisions, random ledger sequences, 24-way parallel overdraw.
- Gaps: (a) `authorize()` only reads a `Snapshot` the caller builds. If a future executor builds it from stale data or outside the allocate lock, limits can be raced; snapshot read and ledger post must share one transaction/lock. (b) Settlement is checked per agent account, not per interaction (SEC-03). (c) `authorize()` trusts that `AgentPolicy` was validated; unvalidated inverted limits are still safe (tested), but zero `max_aggregate_exposure` silently freezes the agent.

### 2. Key custody (D-3, PROPOSED, nothing implemented)
- Threat: one env seed derives every agent key plus a resolver key; leak of the env, logs, a crash dump or a prompt equals total loss of testnet funds and arbitrary escrow resolution.
- Required before implementation: owner approval; testnet-only seed, never on mainnet addresses; `Signer` module the only reader of the seed, no `Debug`/`Serialize` on key types, never logged, never sent to an LLM or the frontend; signer accepts only a typed, already-authorized `PostingSet`/call (not raw calldata) and re-checks recipient is the configured escrow and amount equals the authorized amount; per-agent key derivation (not shared) so one agent key leak is bounded; separate resolver key from agent keys; gas-funding cap.
- Gaps: no secret scanning in CI; `.env.example` and `.gitignore` conventions exist but are unenforced.

### 3. Webhook authenticity (QuickNode Streams, D-16)
- Not implemented. Threats: forged "deposit confirmed"/"resolved" events that make the backend mark funds funded or settled; replay of valid events; reorgs delivering events later undone; duplicate delivery.
- Required: verify QuickNode's HMAC signature (shared secret in env) over the raw body with constant-time compare, reject stale timestamps; treat events as hints and confirm state by reading the contract (`getChallenge`) before any ledger post; idempotency key = `chainId:txHash:logIndex`; wait for finality depth; bind events to the known escrow address and chain ID; body size limit; separate route from the unauthenticated owner API.
- Gap: the owner API has no auth (D-9), so a public tunnel for the webhook would also expose every owner endpoint. Expose only `/webhook` through the tunnel.

### 4. LLM output validation (D-2, D-8)
- Threats: prompt injection via market/event text (Gemini event titles, other agents' negotiation messages) steering intents; malformed JSON; oversize amounts; model "confidence" treated as probability.
- Controls that exist: intents are typed (`Intent`), amounts are strict digit strings, unknown variants rejected, `authorize()` gates everything. Tested that unknown intent types and negative/float amounts fail to parse.
- Required: schema-validate then `authorize()`; never place secrets or keys in prompts; cap number and size of intents per run; counterparty text is data, rendered escaped in the UI; store decision summaries only.
- Gap: the counterparty-supplied terms (resolution source, time window) need allow-listing, not free text, or an injected term could make a challenge unresolvable.

### 5. Agent and user isolation (D-9)
- Single demo user, no authentication or authorization. Any process that can reach `127.0.0.1:8080` can read and mutate everything. Server binds loopback only.
- CSRF (verified, SEC-01): POST endpoints ignore Content-Type, so a hostile page can send a simple `text/plain` cross-origin POST without preflight. CORS does not prevent the write. DNS rebinding is also possible because there is no `Host` check.
- Agent-to-agent: isolation is by account naming in one ledger; there is no per-agent credential. Acceptable for the demo, unacceptable for multi-user.
- Required before any public deployment or real funds: auth (session or token), strict `Content-Type: application/json`, Host/Origin check, rate limits.

### 6. Custom-code sandbox (D-11, design only)
- Threats: CPU/memory exhaustion, escape to host, exfiltrating the seed/DB URL, timing side channels, calling network, producing huge output.
- Design controls (must all be verified by a spike, several details are UNVERIFIED in research): Wasmtime with fuel and epoch deadline, memory cap, no WASI sockets/preopens/env, fresh store per run, output size cap, concurrency semaphore, run in a separate process without access to the DB URL or seed. Output is only a list of `Intent`s validated and passed to `authorize()`; the sandbox must not be given balances beyond a read-only context snapshot.
- Tests to write when built: infinite loop, allocation bomb, deep recursion, giant output, attempts to read env/files/network, malformed intent JSON.

### 7. Escrow (`ChallengeEscrow`, from `contracts/README.md`)
- Resolver trust: a compromised or wrong resolver can award any Active challenge to either party or void it; no disputes. Resolver can resolve at any time (front-running the timeout refund). Resolver can also be a party (tested, SEC-07). `resolveDeadline` has no upper bound, so a resolver can create a challenge whose timeout refund never opens (tested, SEC-04); parties must check terms before depositing.
- Token trust: USDC is upgradeable and blocklistable; blocked parties get deferred payouts (tested with fuzz); if the escrow itself is blocklisted funds are stuck. Fee-on-transfer, false-returning, lying, reentrant tokens are handled for deposits. Only standard USDC supported.
- Donations: tokens sent directly to the escrow are stranded; `balance >= totalHeld`, not `==` as the README states (SEC-06, informational).
- No admin withdrawal, no upgrade, no pause: also no emergency stop.
- Not verified: behaviour on Arc itself (needs `arc-forge`), gas griefing by a returndata-bomb token (token is trusted).

### 8. Secrets handling
- Rules (AGENTS.md 8.1): env vars only, `.env.example` placeholders, never to frontend or LLM. Current state: only `DATABASE_URL`, `PORT`, `CORS_ORIGIN` are read; no secrets in repo. `tracing` logs request metadata via `TraceLayer`; headers are not logged by default, but keep it that way once API keys or webhook signatures exist (never log `Authorization`/signature headers or request bodies).
- Gap: no automated secret scan in the repo.
