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

## From backend M2 (2026-11-15, full table F-1..F-15 with code locations in `tasks/backend.md`) — all `default`, FLAGGED FOR OWNER REVIEW

| ID | Date | Status | Decision | Why / notes |
|---|---|---|---|---|
| D-22 | 2026-11-15 | default, review | Resolution rule: one integer observation; "above" wins at `observed >= threshold`, "below" at `<`, no ties (F-1). Observation = first successful read at or after `end_ts`. Formulas `pm_yes_price_bps_v1` (Gemini prediction-market contract: settled side, else last trade price in bps) and `spot_last_price_micros_v1` (F-2) | Matches Gemini's "or above" strike convention. Backend-administered, not an oracle |
| D-23 | 2026-11-15 | default, review | Missing market data is never a value: retry each tick for 600 s after `end_ts`, then Void: both stakes refunded, no P&L (F-3) | AGENTS.md 8.3/8.4 |
| D-24 | 2026-11-15 | default, review | Equal stakes, winner takes both, no fee (F-4). `start_ts` is the funding deadline (proposal + 300 s); unfunded by then = cancelled and approvals expire (F-5) | Mirrors the escrow contract (D-5) |
| D-25 | 2026-11-15 | default, review | `authorize()` runs at proposal, at each answer and at commit; only commit can raise an approval. Commit authorizes BOTH agents on current policy + fresh snapshot under locks; any deny cancels, nobody is funded alone. Approving re-runs authorization (F-6..F-8) | AGENTS.md 8.2 |
| D-26 | 2026-11-15 | default, review | Exposure = stakes of the agent's funded/active/resolving challenges; unfunded agreements do not count (F-9). One open challenge per agent per market (F-13) | Keeps aggregate-exposure check simple |
| D-27 | 2026-11-15 | default, review | Negotiation capped at 3 offers including the proposal; a counter may change only stake and threshold (F-10) | Bounded negotiation |
| D-28 | 2026-11-15 | default, review | Settlement and refunds are not agent intents and are not re-authorized: a paused or stop-lossed agent's open challenge still settles (F-11) | A pause must not strand the counterparty's funds |
| D-29 | 2026-11-15 | default, review | Market choice: open Gemini prediction-market contract on the asset priced closest to 0.50 in the soonest event after the funding deadline, else spot price labelled as fallback (F-14). Supports D-14 (still the owner's call) | Gemini bounty needs prediction-market APIs |
| D-30 | 2026-11-15 | default, review | Security fixes: body-carrying requests must be `application/json` (415), so approve/reject/tick need a `{}` body; settle/refund ledger keys derive from the interaction with unique indexes (SEC-01, SEC-03) | `reports/security-m1.md` |
| D-31 | 2026-11-15 | default, review | Grok reasoner is wired but only active when `XAI_API_KEY` is set, with rule-based fallback on any error. Tested against a local mock only; endpoint path and model name `grok-4.7` are UNVERIFIED. D-8 remains the owner's decision | No key exists |
| D-32 | 2026-11-15 | default, review | Escrow deadline caps: `MAX_FUND_WINDOW` = 7 days from `create` to `fundDeadline`, `MAX_RESOLVE_WINDOW` = 30 days from `fundDeadline` to `resolveDeadline`; refund is open at most 37 days + 1 h after `create`. The resolver address cannot be a party (SEC-04, SEC-07). Constants, local only, not deployed | `reports/security-m1.md`. Backend chain executor (not built) must keep deadlines inside both windows and never use the resolver as an agent wallet |
| D-33 | 2026-11-15 | default, review | Proposal cooldown: 600 s per (proposing agent, market), counted from the proposal, whatever became of it; `PROPOSAL_COOLDOWN_SECS`, `0` disables. Enforced in the runtime before any reasoning. Denied or no-counterparty proposals are not rate-limited yet (F-16) | Stops the propose/cancel loop on every tick; longer than the 300 s funding window |
| D-34 | 2026-11-15 | default, review | Dev clock guard: `/dev/tick` with a simulated `now` past the funding deadline of an unfunded challenge returns 409 unless `allow_expiry: true`. The deadline rule (D-24) is unchanged. A proposer sizes its stake inside its own automatic limit, so approvals arise on the counterparty side: by design, not changed (F-17) | Demo reliability without weakening a rule |
| D-35 | 2026-11-15 | default, review | Demo seed `POST /api/dev/seed-demo` (dev endpoints only): two fixed agents, 100 simulated USDC each, skeptic `approval_threshold` 1.00; goes through normal validation and ledger paths, idempotent (F-18) | Deterministic demo on a fresh db |
| D-36 | 2026-11-15 | default, review | Co-captain suggestions (rule-based; Grok only on `POST /suggestions/generate` when `XAI_API_KEY` is set, mock-tested, endpoint/model UNVERIFIED). Rules and thresholds: approval expiring within 120 s; idle funds (>= 1 USDC idle 3600 s, or unfunded agent); 3 straight losses or loss >= 20% of allocation → halve max stake; 3 denials for one reason → raise that limit; last 3 approvals all approved → raise `approval_threshold` to the largest approved stake; halted agent explained (F-19..F-23, F-26) | Advisory only. Details and formulas: `tasks/backend.md`, `cocaptain.rs` |
| D-37 | 2026-11-15 | default, review | A suggestion may change only `max_per_interaction`, `max_aggregate_exposure`, `approval_threshold`, `min_conviction_bps`, `stop_loss` (set, never remove) and `risk_tolerance`. Accept merges it into the current version and goes through the same validated, versioned edit as the owner's; the server computes and shows `loosens_control`. Statuses pending / accepted / dismissed / expired; no repeat until the subject changes (F-24, F-25) | AGENTS.md 2.3, 8.2: suggestions are never authorization and never touch the ledger |
| D-38 | 2026-11-16 | default, review | A proposal attempt denied by `authorize` or without a counterparty starts the same `PROPOSAL_COOLDOWN_SECS` (600 s) cooldown per (agent, market) as a proposal, counted from the run's `ts` (lifecycle clock), only while the agent's config version is unchanged. Reasoner errors and invalid terms are not rate-limited (a transient error should retry). Supersedes the "not rate-limited yet" note in D-33 (`lifecycle.rs`) | Stops a blocked agent re-reasoning (and calling Grok) every 15 s tick; an owner edit is the usual fix for a denial, so it should take effect on the next tick |
| D-39 | 2026-11-16 | default, review | P&L over time is plotted against the settlement time on the lifecycle clock (`interactions.settled_at`), not the ledger rows' wall-clock `ts`; rows settled before migration 0005 use `observed_at` (same value) (`interactions.rs`) | Charts of a simulated-clock demo would otherwise bunch every point at the moment the script ran |
