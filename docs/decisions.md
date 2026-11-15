# Decisions

Format: ID, date, status (`proposed` = awaiting owner, `default` = proceeding on testnet per AGENTS.md 7.6, owner may overturn, `accepted`), decision, why.

| ID | Date | Status | Decision | Why |
|---|---|---|---|---|
| D-1 | 2026-11-14 | accepted (kickoff) | Rust backend (Axum, Tokio, SQLx, Alloy), Next.js frontend, Solidity + Foundry, PostgreSQL | Mandated by the kickoff prompt; supersedes the TypeScript-backend suggestion in AGENTS.md 3.4 |
| D-2 | 2026-11-14 | default | All reasoning (blocks, LLM, custom code, co-captain) ends in typed `Intent`s; a pure `authorize()` in `core` is the only gate to the executor | Keeps AI separate from authorization; makes the builder and custom code safe to add later |
| D-3 | 2026-11-14 | **proposed, blocks chain work** | Custody: one EVM address per agent on Arc testnet, keys derived from a testnet-only seed held in a backend env var, used only by an isolated `Signer` module | Agents genuinely hold and move their own funds onchain, good for the Arc and QuickNode stories. Custody is on the stop-and-wait list (AGENTS.md 7.6). Alternative: one operator wallet + internal ledger only (simpler, weaker demo). Alternative: Circle developer-controlled wallets (stronger Arc story, more integration work) |
| D-4 | 2026-11-14 | default | Double-entry ledger in Postgres, integer USDC minor units, every row tagged `arc_testnet` or `simulated` | Prevents double counting; makes P&L formulas auditable |
| D-5 | 2026-11-14 | default | Escrow: equal stakes, both must deposit by a funding deadline or anyone can refund; resolver-only `resolve`; refund if unresolved after deadline + grace; no disputes in MVP; no admin withdrawal | Smallest contract that covers lock, release, refund, timeout |
| D-6 | 2026-11-14 | default | Resolution source is the Gemini exchange public price, read by the backend, labelled "backend-administered" in the UI | A market data API is not an oracle; we say so |
| D-7 | 2026-11-14 | default | Agent config, policy and graph are stored as immutable versions; interactions reference a version | Editing must not rewrite history |
| D-8 | 2026-11-14 | **proposed** | Agent reasoning uses xAI Grok behind a `Reasoner` trait from M2, with a deterministic rule-based fallback so the lifecycle and tests run without it. Co-captain stays deferred | AGENTS.md puts Grok reasoning in Priority 2; the kickoff says Grok is not an MVP dependency. The trait + fallback satisfies both: Grok is used but not depended on |
| D-9 | 2026-11-14 | default | Single demo user, no auth, for the hackathon; documented as a known limitation | Time |
| D-10 | 2026-11-14 | default | Behavior-graph schema is defined in Rust and exported as JSON Schema; the frontend renders from it | Keeps the block schema independent of the UI |

Research-dependent decisions (sandbox engine, builder library, Arc specifics, Streams fallback) are appended below once `research/` is reviewed.


## From research (2026-11-14, source: `research/runtime-and-sandbox.md`)

| ID | Date | Status | Decision | Why |
|---|---|---|---|---|
| D-11 | 2026-11-14 | proposed | Custom code: users write JavaScript defining `decide(ctx)`; it runs in a QuickJS guest (built once with Javy) inside Wasmtime. JSON context on stdin, JSON list of proposed intents on stdout. Limits: fuel ~50M, epoch deadline ~500 ms, 32 MiB memory, no WASI sockets or preopens, fresh Store per run, concurrency semaphore. Fallback if time runs out: Rhai in a separate worker process | JS is the language most users can write; Wasmtime gives CPU, memory and capability limits. Several API details in the research file are still marked UNVERIFIED and must be confirmed with a spike before M5 |
| D-12 | 2026-11-14 | default | Visual builder uses React Flow (`@xyflow/react`, MIT). The canvas state is converted to our own graph schema before saving; React Flow's `toObject()` output is never persisted | Keeps the stored schema independent of the UI library |
| D-13 | 2026-11-14 | default | Toolchain floor is the Rust version required by Wasmtime (reported as 1.97); pin crate versions from the research file at scaffold time and verify with `cargo build`. Share types to the frontend with `ts-rs`. Money in JSON is a string of integer minor units | Avoids version drift from older tutorials (alloy 2.x, sqlx 0.9, axum 0.8 path syntax, schemars 1.x) |


## From research (2026-11-14, source: `research/sponsors.md`)

| ID | Date | Status | Decision | Why |
|---|---|---|---|---|
| D-14 | 2026-11-14 | **proposed** | Challenges are built on Gemini **prediction-market** events, not only spot prices. Agents disagree about an event contract; terms reference the Gemini event/contract id. Two resolution formulas: (a) contract price at `end_ts` vs a threshold (short horizon, used in the demo), (b) the event's published outcome. Spot ticker stays as a secondary discovery signal | The Gemini bounty ($4k, the largest) requires projects "built on Gemini's prediction market APIs"; spot data alone likely does not qualify. Response schemas are not yet read: spike first |
| D-15 | 2026-11-14 | default | Arc testnet: chain ID 5042002, RPC `https://rpc.testnet.arc.io`, USDC at `0x3600000000000000000000000000000000000000` (native gas token: 18 decimals native, 6 via ERC-20). All contract and ledger amounts use the 6-decimal ERC-20 interface only; never `msg.value` | Avoids the 18/6 decimals trap. Arc needs its own Foundry fork (`arc-forge`); install source still unverified |
| D-16 | 2026-11-14 | default | QuickNode Streams (supports Arc testnet) is the primary source of escrow events, delivered by webhook; RPC log polling is only a reconciliation fallback | Bounty requires Streams to be "a core data source". Needs a public webhook URL (tunnel in dev) and a QuickNode plan |
| D-17 | 2026-11-14 | proposed | Enter main track "Agentic Payments & AI" plus the Arc chain award and all four bounties | Closest fit to "AI agents with wallets, budgets, guardrails" |
| D-18 | 2026-11-14 | proposed | Hard deadline is Devpost Fri Nov 20, 12 PM CST (per the event page); keep Wed Nov 18 EOD as the internal demo-working target. `AGENTS.md` 1.4 still says Nov 19 and is the owner's to update | Research found a different date than AGENTS.md |


## From contracts M1 (2026-11-14, detail in `tasks/contracts.md` and `contracts/README.md`)

| ID | Date | Status | Decision | Why |
|---|---|---|---|---|
| D-19 | 2026-11-14 | default | `create` is resolver-only; parties consent by depositing. `resolve` allowed any time while Active (nothing onchain prevents an early resolve); `resolveVoid` also cancels before full funding; `GRACE` = 1 hour; resolver immutable | Smallest trust surface that still lets the backend cancel. The early-resolve power is a stated trust assumption |
| D-20 | 2026-11-14 | default | If a payout transfer fails (e.g. USDC blocklist), the amount is parked and collected later with `claim(id, party)`; events `PayoutDeferred` and `Claimed`. Deposits must raise the balance by exactly the stake | One blocked party cannot freeze the other's funds. The backend indexer must handle the two extra events |
| D-21 | 2026-11-14 | open risk | Arc USDC is an upgradeable proxy with a blocklist; `evm_version = prague` unverified on Arc; Arc deploy steps untested | Verify at first testnet deploy |
