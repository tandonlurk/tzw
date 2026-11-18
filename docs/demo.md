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

## Full README demo after the M3 merges (verified by hand 2026-11-16, backend 332 tests)

Lead re-ran every demo section of `backend/README.md` verbatim on fresh databases (port 8095, `MARKET_DATA=fake TICK_INTERVAL_SECS=0`, `AEN_SANDBOX_MODULE` set). All outputs matched the README comments.

| Section | Observed |
|---|---|
| Challenge + co-captain | as above: bull +7.50 / skeptic -7.50 over 3 rounds; `lower_max_stake` accepted → skeptic v2, max 5.00 |
| Collaboration (same db) | accept → `["fund","cancel"]`; fund → `approval_required`; fund with `owner_approved` → `funded`; complete → net `+1.80` / `-1.80` (60/40 of an 8.00 pool); six readable activity rows |
| Custom code (fresh db) | broken code refused on save (422 `custom_code_failed`); test-run at +7%: stake 20.00 `deny` `exceeds_max_per_interaction`; at +3%: 8.00 `require_approval`, logs `move 300 claim above`, fuel 1,252,292; infinite loop → `out_of_fuel`; tick → proposed 1, accepted 1, approvals 2; both approved → `active` |

Web views for all of the above were browser-checked by the web specialist (`reports/qa-m2-web.md`); the custom-code editor was browser-checked too.

## Testnet (Arc) demo: live Gemini data, autonomous scheduler (verified 2026-11-17/18)

Real Arc testnet USDC (testnet asset, no real value) in two agent vaults, real Gemini exchange prediction-market data (`MARKET_DATA=gemini`, the default), the autonomous scheduler (`TICK_INTERVAL_SECS=10`). The only manual steps are the owner's: one config edit and one approval. Resolution is backend-administered: the backend reads Gemini and the resolver key calls the escrow. Full run, tx hashes and findings: `reports/handoffs/2026-11-17-testnet-gemini.md`.

Needs: testnet mnemonic in `backend/.env` (never printed), the test owner (index 2) funded from the faucet (`docs/setup/arc-testnet-funding.md`), a fresh database, `jq`, Foundry's `cast` for the final withdraw.

```sh
createdb aen_arc_gemini
cd backend; set -a; source .env; set +a      # AEN_TESTNET_MNEMONIC only
export DATABASE_URL="postgres://$USER@localhost/aen_arc_gemini" MARKET_DATA=gemini FUNDING_WINDOW_SECS=180 \
  ARC_RPC_URL=https://rpc.testnet.arc.io ARC_ESCROW_ADDRESS=0x4179f2e314410D10C2906B1914bAAe8DD93b5698 \
  ARC_VAULT_FACTORY_ADDRESS=0x4EA6ac6A9F7b8fcf01ba04b177F0A098d9664C45 AEN_DEV_CHAIN_OWNER=1 DEV_ENDPOINTS=true
TICK_INTERVAL_SECS=0 cargo run -p aen-server 2>&1 | grep -vi mnemonic &     # scheduler off while setting up
B=localhost:8080/api; H='content-type: application/json'; BULL=00000000-0000-4000-8000-0000000000d1
curl -s -XPOST -H "$H" $B/dev/seed-demo -d '{}'
curl -s -XPOST -H "$H" $B/dev/chain/setup-demo-vaults -d '{"amount":"2500000"}'   # 2 vault creates + 2 transfers of 2.50
```

**Owner config edit (through the API, no code change).** The seeded bull fires only on a 2% BTC move in an hour and proposes only at 60% conviction; its rule-based conviction is `5000 + 5 × move_bps`, so on a quiet market it never proposes. For the demo: trigger `threshold_bps` 200 → 1, conviction gate `min_bps` 6000 → 5000 (policy `min_conviction_bps` to match), and `horizon_secs` so the challenge ends a minute or two **after** an hourly expiry (Gemini settles its BTC events about 6 s after expiry, so the challenge resolves on Gemini's published outcome instead of a possibly stale last trade):

```sh
curl -s $B/agents/$BULL/versions | jq 'max_by(.version).graph
  | .nodes |= map(if .id=="trigger" then .params.threshold_bps=1 elif .id=="conviction" then .params.min_bps=5000 else . end)' > g.json
HZ=$(( $(date -j -u -f %Y-%m-%dT%H:%M:%S <next hour>:01:30 +%s) - $(date -u +%s) - 15 - 180 ))
curl -s $B/agents/$BULL | jq --slurpfile g g.json --argjson hz $HZ \
  '{name, description, config: (.config|.horizon_secs=$hz), policy: (.policy|.min_conviction_bps=5000), graph: $g[0]}' \
  | curl -s -XPUT -H "$H" $B/agents/$BULL -d @- | jq -c '{version, horizon: .config.horizon_secs}'
# restart the server with TICK_INTERVAL_SECS=10 (same env)
```

| Step | Who | Observed in the run |
|---|---|---|
| Discover + propose | scheduler | Bull read BTC +0.22% over 1 h and the soonest-expiring traded BTC contract, `GEMI-BTC15M2611180000-UP` ("BTC price today at 7pm EST: Up", last trade 0.14). Claim: YES price at or above 0.14 at 00:01:54 UTC, 2.50 per side |
| Negotiate | scheduler | Skeptic (contrarian, 51% conviction < its 70% gate) countered 1.25 per side; bull accepted on the next tick |
| Authorize | scheduler | 1.25 is above the skeptic's automatic limit of 1.00: approval requested, nothing reserved |
| Approve | owner | `POST /approvals/<id>/approve` → `funded` (both stakes reserved in the ledger) |
| Fund | scheduler, next tick | escrow `create` (resolver key), bull vault `stake`, skeptic vault `stake` (operator key) → `active`, ~3.5 s for 3 txs |
| Resolve + settle + release | scheduler, first tick after `end_ts` (00:01:54) | Gemini settled the event at 00:00:08 UTC: `resolutionSide: "no"` (index 64,874.75 vs reference 64,923.72). At 00:02:02 the backend read it (observed 0.00 YES price, `contract resolved no`), called `resolve(skeptic vault)`, settled the ledger (bull -1.25, skeptic +1.25) and released both vaults' exposure (2 `release` txs) |
| Check | `GET /agents/<id>/vault` | Bull vault onchain 1.25 = ledger 1.25, skeptic onchain 3.75 = ledger 3.75, exposure 0, `drift` 0 on both. The owner then withdrew both balances (`cast send <vault> "withdraw(address,uint256)" <owner> <amount>` with the test-owner key) and posted each tx to `POST /agents/<id>/vault/sync`: both vaults 0 onchain = 0 ledger, drift 0 |

The owner paused the bull once the challenge was active (`POST /agents/$BULL/status {"status":"paused"}`) so it would not open a second challenge on the next 15-minute contract; pausing stops discovery and new commits only, never resolution.

Caveats seen in this run: `GET /stats/portfolio` (the Dashboard) covers simulated funds only, so the testnet result shows on the agents (`stats.realized_pnl`: bull -1.25, skeptic +1.25), the challenge and Activity, not on the dashboard chart. The two vault `release` txs are not recorded by the server (found on the explorer under the operator address). Discovery picks the soonest-expiring traded contract, so with a 180 s funding window it chose the 15-minute up/down contract, not the hourly ladder that expires at the same time. Proposal to settlement took 14.5 min; total spend about 0.09 testnet USDC in gas (2 vault creates are most of it); the 5.00 staked came back to the owner.
