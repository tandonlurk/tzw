# Demo plan

Goal: one coherent lifecycle, about 4 minutes, on Arc testnet with real testnet USDC. Anything simulated is labelled on screen.

| # | Step | What the audience sees | Proves |
|---|---|---|---|
| 1 | Create agent "Bull" from a template | Config form: goals, risk, limits (max 5 USDC per challenge, approval above 3) | Config-based creation |
| 2 | Open the logic builder | Blocks: market move → evaluate → conviction ≥ 0.6 → propose challenge. Edit a threshold, validate, save | Visual builder, versioned graph |
| 3 | Show second agent "Skeptic" (pre-made, different owner settings) | Its own wallet address, balance, strategy | Persistent identity + wallet context |
| 4 | Trigger a tick | Both agents read the Gemini exchange BTC price and form opposite views | Independent discovery |
| 5 | Negotiation | Activity feed: proposal, counter-offer on stake/threshold, acceptance, explicit terms | Negotiated measurable challenge |
| 6 | Permission check | Bull's stake exceeds its auto limit → approval card; owner approves. Also show a denied over-limit attempt | Enforced limits outside the model |
| 7 | Funding | Two deposit transactions, explorer links, events arriving via QuickNode Streams | Onchain escrow, event monitoring |
| 8 | Resolution | Short horizon (about 2–3 min) elapses, backend reads price, calls resolve; labelled "backend-administered" | Monitoring + settlement |
| 9 | Results | Balances, realized P&L, win rate, history and dashboard chart update | Accounting |
| 10 | (If built) Custom-code agent | Paste a small strategy, test-run in sandbox, show it proposing and being limited by policy | Custom code cannot bypass permissions |
| 11 | (If built) Co-captain | Grok suggests a limit change; owner accepts | Advisory layer |

Fallbacks: a recorded run of steps 7–9 in case testnet or faucet is down; a `simulated` environment that runs the same lifecycle against the ledger only, clearly labelled.

## Simulated run that works today (verified by hand 2026-11-15, `recovery` after the backend-m2b merge)

Exact commands: `backend/README.md`, "Demo script" (fresh db, `MARKET_DATA=fake TICK_INTERVAL_SECS=0`, `POST /api/dev/seed-demo`). Watch it in the web app under Activity, Approvals, Challenges and Dashboard. All funds are simulated; nothing signs or sends a transaction, and resolution is backend-administered on a fake market.

| Step | Call | Observed |
|---|---|---|
| Seed | `POST /dev/seed-demo {}` | "Demo Bull (simulated)" and "Demo Skeptic (simulated)", 100.00 simulated USDC each; a second call creates nothing |
| Market | `POST /dev/fake-market {"move_bps":300,"yes_price_bps":5500,"resolution_side":"open"}` | BTC +3%, contract at 0.55 |
| Tick 1 | `POST /dev/tick {}` | proposed 1, countered 1: bull stakes 5.00 on "at or above 0.55", skeptic counters 2.50 per side |
| Tick 2 | `POST /dev/tick {}` | accepted 1, approvals_requested 1: skeptic's stake 2.50 is above its automatic limit of 1.00, nothing funded |
| Guard | `POST /dev/tick {"now": <after end>}` before funding | 409 `clock_jump_would_expire` |
| Approve | `POST /approvals/:id/approve {}` | interaction `active`, both stakes committed together |
| Tick 3 | `POST /dev/tick {}` | no new proposal (one open challenge per market, then the 600 s cooldown) |
| Resolve | `POST /dev/fake-market {"yes_price_bps":7000}`, then `POST /dev/tick {"now": end + 5 s}` | resolving 1, settled 1; observed 7000, outcome `a` (bull) |
| Result | `GET /stats/portfolio` | bull realized P&L +2.50, 1 win; skeptic -2.50, 1 loss; totals: allocated 200.00, committed 0, portfolio realized P&L 0 |

Also checked: a `text/plain` POST to approve returns 415.

Caveats: deterministic only on a fresh database; a rerun within 600 s needs `PROPOSAL_COOLDOWN_SECS=0`; a slow live demo needs a longer `FUNDING_WINDOW_SECS` (300 s default, approvals included).

## Co-captain step (verified by hand 2026-11-16, `recovery` after the backend-m3 stage 1 merge)

Lead re-ran the whole README "Demo script" on a fresh db (port 8095). Steps above reproduced unchanged, then the co-captain rounds:

| Step | Observed |
|---|---|
| Round 2 (simulated clock) | proposed 1 / countered 1, accepted 1 / approvals_requested 1, approve → active, settle: `{"settled":1,"suggestions":0}` |
| Round 3 | same, settle: `{"settled":1,"suggestions":2}` (skeptic's third straight loss) |
| Pending suggestions | skeptic `raise_approval_threshold` → `approval_threshold` 2.50; skeptic `lower_max_stake` → `max_per_interaction` 5.00 (rule-generated, no model call) |
| Accept `lower_max_stake` | `"accepted"`; skeptic now version 2, `max_per_interaction` 5000000 |
| Portfolio | bull realized +7.50, 3 settled, 3 wins; skeptic -7.50, 3 settled, 0 wins (simulated) |

`pnl_over_time` points now carry the simulated settlement times (D-39), so the dashboard chart spreads the three rounds over the simulated hours.
