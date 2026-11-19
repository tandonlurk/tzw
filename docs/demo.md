# Demo plan

Goal: one coherent lifecycle, about 4 minutes, on Arc testnet with real testnet USDC. Anything simulated is labelled on screen.

> **UI renamed 2026-11-19 (branch `ui-1119`, D-72).** Dashboard → **Overview** `/` (Basic) or **Terminal** `/advanced` (Advanced; its Simulated / Arc testnet switch replaces the second dashboard panel, and Overview shows an "Onchain agents" strip when a vault exists). "Dev: run tick" → the dashed **Dev · Run tick** control (Overview, Terminal, Approvals, Challenges). Interactions → **Challenges**. Logic builder and Edit → the agent's **Logic** and **Configuration** tabs. The mode switch is the icon in the top right (⌘\\).

| # | Step | What the audience sees | Proves |
|---|---|---|---|
| 1 | Create agent "Bull" from a template | Config form: goals, risk, limits (max 5 USDC per challenge, approval above 3) | Config-based creation |
| 2 | Open the agent's Logic tab | Blocks: market move → evaluate → conviction ≥ 0.6 → propose challenge. Edit a threshold, validate, save | Visual builder, versioned graph |
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

## Rerun after starter templates gained approach nodes (verified 2026-11-18, overnight item 4, backend 392 tests)

Templates now carry an `approach.profile` node (D-69). Lead re-ran `backend/README.md` "Demo script", the co-captain rounds and the collaboration step on a fresh db (`aen_item4_demo`, port 8097, `DEV_ENDPOINTS=true MARKET_DATA=fake TICK_INTERVAL_SECS=0`), then "Firm demo" on another fresh db (`aen_item4_firm`). Every number is unchanged:

| Step | Observed |
|---|---|
| Templates | bull collab 15 % / ambition 100 %, skeptic 25 % / 50 %, cautious 30 % / 20 % |
| Tick 1 | proposed 1, countered 1: bull offers 5.00 (its stake target is 10.00, so not lowered), skeptic counters 2.50 |
| Tick 2, guard, approve | accepted 1, approvals_requested 1, funded 0; early clock jump 409 `clock_jump_would_expire`; approve → `active` at 2.50 |
| Tick 3, resolve | nothing new; resolving 1, settled 1, outcome `a`, observed 7000 |
| Portfolio | bull +2.50 / 1 win, skeptic -2.50; allocated 200.00, committed 0, portfolio realized 0 |
| Co-captain rounds | `{"settled":1,"suggestions":0}`, then `{"settled":1,"suggestions":2}`; `lower_max_stake` accepted → skeptic v2, max 5.00; bull +7.50 / 3 / 3, skeptic -7.50 / 3 / 0 |
| Collaboration | accept → `["fund","cancel"]`, fund → `approval_required`, owner-approved → `funded`, complete → `+1.80` / `-1.80` |
| Firm demo | tick 1 `{"proposed":1,"firm_contributions":2,"accepted":1,"approvals_requested":1}`; rival approval 5.00; settle → lead +3.80, paid to members 1.20 (scout 0.70, analyst 0.50); balances 100.70 / 100.50 / 95.00 / 103.80; one approach node per agent |
| New agent from "Cautious" as-is | created with its approach node; its stake target is 0.72 (36 % of 2.00), see the `owner_approval_flow_through_http` test |

All simulated; nothing signed or sent.

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

## Testnet video script (for recording; rehearsed live on Arc 2026-11-18, overnight item 5)

One take, about 6 minutes of wall clock (cut the 4-minute wait). Real Arc testnet USDC (no real value) in two agent vaults, real escrow, explorer links. Every step below was run on Arc at 06:19–06:24 UTC on 18 Nov with these exact commands (fresh db `aen_item5_arc`, port 8098 instead of 8080) and produced the numbers shown.

### Pick the market path at T-15 min

| Check | Command | Path |
|---|---|---|
| Arc RPC up | `curl -s -XPOST -H 'content-type: application/json' https://rpc.testnet.arc.io -d '{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}'` → `"result":"0x4cef52"` (5042002) | needed for both |
| Gemini prediction markets up | `curl -s -o /dev/null -w '%{http_code}\n' https://api.gemini.com/v1/prediction-markets/events` → `200` | `200` and you can wait for an hourly expiry → **B**; otherwise **A** |
| Test owner has ≥ 5.2 USDC | `cast call 0x3600000000000000000000000000000000000000 "balanceOf(address)(uint256)" 0x1F0eb73C714Ae0BD1a97c1c8e0F34513073393Fb --rpc-url https://rpc.testnet.arc.io` (6 decimals) → was 13672617 = 13.67 after the rehearsal | below 5.2: faucet (`docs/setup/arc-testnet-funding.md`) |

- **A (default, rehearsed): Arc testnet + fake market feed, manual ticks.** Deterministic, ~4 min from proposal to settlement. Say on camera (and caption): "market data here is a fake feed so the outcome is controllable; money and escrow are real Arc testnet". The Gemini prediction-markets API returned HTTP 500 on every query from 05:44 to 06:18 UTC on 18 Nov, and BTC spot `last` stayed at 64,321.41 for 30+ min, so a live-data take tonight would have resolved on a stale price.
- **B: Arc testnet + live Gemini data, autonomous scheduler.** Follow the previous section ("Testnet (Arc) demo"). Shows Gemini as the data and resolution source, but takes ~15 min and the outcome is whatever Gemini settles. Only when the check above returns 200.

### Setup (off camera, path A)

```sh
createdb aen_video                                   # fresh db: the numbers below assume it
cd backend; set -a; source .env; set +a              # AEN_TESTNET_MNEMONIC only; never echo it
export DATABASE_URL="postgres://$USER@localhost/aen_video" MARKET_DATA=fake TICK_INTERVAL_SECS=0 FUNDING_WINDOW_SECS=180 \
  DEV_ENDPOINTS=true AEN_DEV_CHAIN_OWNER=1 ARC_RPC_URL=https://rpc.testnet.arc.io \
  ARC_ESCROW_ADDRESS=0x4179f2e314410D10C2906B1914bAAe8DD93b5698 ARC_VAULT_FACTORY_ADDRESS=0x4EA6ac6A9F7b8fcf01ba04b177F0A098d9664C45
cargo run -p aen-server 2>&1 | grep -vi mnemonic &   # port 8080; refuses to start if the RPC is down
(cd ../web && npm run dev) &                         # http://localhost:3000
B=localhost:8080/api; H='content-type: application/json'; BULL=00000000-0000-4000-8000-0000000000d1
curl -s $B/chain | jq -c '{enabled, chain_id}'       # {"enabled":true,"chain_id":5042002}
curl -s -XPOST -H "$H" $B/dev/seed-demo -d '{}'
curl -s -XPOST -H "$H" $B/dev/chain/setup-demo-vaults -d '{"amount":"2500000"}'   # ~10 s: 2 vault creates + 2 transfers of 2.50
curl -s $B/agents/$BULL/versions | jq 'max_by(.version).graph' > g.json
curl -s $B/agents/$BULL | jq --slurpfile g g.json '{name, description, config: (.config|.horizon_secs=60), policy, graph: $g[0]}' \
  | curl -s -XPUT -H "$H" $B/agents/$BULL -d @- | jq -c '{version, env}'          # {"version":3,"env":"arc_testnet"}
curl -s -XPOST -H "$H" $B/dev/fake-market -d '{"move_bps":300,"yes_price_bps":5500,"resolution_side":"open"}'
```

Open browser tabs: Dashboard `/`, My Agents → Demo Bull, Approvals, Interactions, Activity, Wallet, and the explorer `https://explorer.testnet.arc.io`. Ticks are the **"Dev: run tick"** button on Dashboard, Interactions or Approvals (same as `POST /dev/tick`). The challenge ends 240 s after tick 1 (`end_ts` = proposal + 180 s funding window + 60 s horizon).

### Shot list

| # | Do | Screen | What it shows (rehearsal values) |
|---|---|---|---|
| 1 | Open My Agents → "Demo Bull (Arc testnet)" | Agent detail: `Arc testnet` env badge, "Arc testnet vault" panel | Vault address (explorer link), owner = test owner, operator = backend, onchain 2.50 = ledger 2.50, drift 0, caps 2.50 per challenge / 2.50 exposure. Point out: owner can pause, withdraw, revoke the operator |
| 2 | Scroll to config / Edit | Config + policy | Max per challenge, approval threshold, version 3 (horizon edit). "Limits are checked outside the model" |
| 3 | Same for "Demo Skeptic (Arc testnet)" | Agent detail | Its own vault, automatic limit 1.00 USDC |
| 4 | Dashboard → **Dev: run tick** | Tick result line `proposed: 1, countered: 1` | Bull proposes "[fake] BTC price in one hour: Up at or above 0.55", 2.50 each; skeptic counters 1.25 (offer 2 of 3) |
| 5 | **Dev: run tick** | `accepted: 1, approvals_requested: 1` | Bull accepts 1.25; skeptic's 1.25 is above its 1.00 limit, so nothing moves without the owner |
| 6 | Approvals → approve the skeptic card | Approval card: "The stake is above this agent's automatic limit of 1.00 USDC." | Interaction goes `funded` (reserved in the ledger, not yet onchain) |
| 7 | Interactions → **Dev: run tick** (~4.5 s, 3 txs) | `activated: 1` (`funded` stays 0, cosmetic) | Interaction detail: state `active`, Terms, Negotiation, Approvals, Timeline. The stake txs are not rendered on this page yet (they are in the API's `escrow` field); show them from Activity (next shot) |
| 8 | Activity | Readable rows | `escrow_created` (1.25 per side, funding open until …), two `stake_deposited`, two `stake_committed` ("locked in the Arc testnet escrow"). Expand `stake_deposited`: its detail has `explorer_url`; open it → the vault's stake into escrow `0x4179…5698` |
| 9 | Agent detail (bull) vault panel | onchain 1.25, exposure 1.25, ledger committed 1.25, drift 0 | Money is in escrow, not in the vault |
| 10 | Off camera: `curl -s -XPOST -H "$H" $B/dev/fake-market -d '{"yes_price_bps":7000}'`; wait until `end_ts` (a tick before it does nothing) | — | Cut |
| 11 | **Dev: run tick** after `end_ts` (~4 s) | `resolving: 1, settled: 1` | Interaction `settled`, outcome `a` (bull), observed 0.70 vs 0.55, with the page's own note "resolution is backend-administered, not trustless". Its Ledger entries now carry tx hash links (settle rows) |
| 12 | Activity | Two `challenge_settled` rows ("Funds moved onchain through the Arc testnet escrow"), two `vault_released` rows | Expand a `vault_released` row for its `explorer_url` |
| 13 | Dashboard | Second (testnet) "Challenges and performance" panel | Bull +1.25, 1 win; skeptic −1.25; allocated 5.00, committed 0, portfolio realized 0 (one owner on both sides). Simulated panel shows the seed's 100.00 simulated each, labelled |
| 14 | Agent detail vaults | bull onchain 3.75 = ledger 3.75, skeptic 1.25 = 1.25, exposure 0, drift 0 | Ledger reconciles with the chain |
| 15 | (Optional) Wallet page | "Arc testnet" section, per-agent vault balances | Connected-wallet part needs MetaMask on Arc testnet; skip if not set up |

Rehearsal txs (`https://explorer.testnet.arc.io/tx/<hash>`), usable as a fallback cutaway:

| Step | Tx |
|---|---|
| Bull vault create + link / fund 2.50 | 0x528b45269c9360e41bcabcd076720c556fdc8f8fd3bb66451e8af9a07bd65dd8 / 0x6abb1d416a88e7534da5b79a28cfe7f8598db21f40331d631e6b095d16ad7230 |
| Skeptic vault create + link / fund 2.50 | 0xa16e6f770b7d85996f85176d25ae7f8bf89d7a3b01e3c7c60d6f58c202c97b8c / 0xc55c22e1ace8df75616f125e9113ec11bdef87d7c039d9bdc34ab0a01fb1f260 |
| Escrow create | 0x8c3865598eb5621178f09717e739f020615df80c0c076c4ac427c49749a4c2b1 |
| Bull / skeptic stake 1.25 | 0xf963437dda4a611127b2e4e6436312e834160ae892f193cc66cd3bec20d355ae / 0xe98b861d79ead2c5ed38db556cd397be245f724a8f3dc14ec03182a446be72bb |
| Resolve → bull | 0xf5c622f8075800e63628417202da335ec249f76b8a86b37b2af2cbe8275624a3 |
| Bull / skeptic vault release | 0xfaa1e822c34c1995564cf7b0ba226c9737d53f21ebf76580045c48e358625863 / 0xcce32eb5382a4c568d9c6926d7c21ffe069dacff5d7440ac19f61a0c97567529 |
| Withdraw 3.75 / 1.25 to the owner | 0xc021606ebd6df7963a78815c300a270b57d81b726e541b623b22f400ad177238 / 0xfd9d7c62225f07c00ae2241f01a7cdcd781e29828f40775ff77d7e5b167bf9f9 |

Vaults: bull `0x3b5b7488bf3a6f6CACf4aA43bf135734745157bE`, skeptic `0x83903a230f64627B4E4Dd24c7309ef931D0f1d75`. Gas: test owner 0.055 USDC (two vault creates, two transfers, two withdraws); resolver and operator a few cents more. No server warnings or errors.

### Cleanup (off camera)

Withdraw both vaults to the test owner and sync, so the 5.00 comes back:

```sh
OWNER=0x1F0eb73C714Ae0BD1a97c1c8e0F34513073393Fb; RPC=https://rpc.testnet.arc.io
TX=$(cast send <vault> "withdraw(address,uint256)" $OWNER <onchain_balance> --mnemonic "$AEN_TESTNET_MNEMONIC" --mnemonic-index 2 --rpc-url $RPC --json | jq -r .transactionHash)
curl -s -XPOST -H "$H" $B/agents/<id>/vault/sync -d "{\"tx_hash\":\"$TX\"}"   # then GET .../vault: 0 = 0, drift 0
```

### If something goes wrong

| Symptom | Cause | Do |
|---|---|---|
| Server exits at start | RPC unreachable, wrong chain id or resolver | Re-run the RPC check; wait or switch to the recorded fallback below |
| Tick shows `errors: [... chain_unavailable ...]` or a tick takes > 30 s | Arc RPC slow or rate-limited | Click tick again. Every chain step reads the escrow first, so a retry never double-stakes or double-resolves. The 180 s funding window covers about 3 min of retries |
| Interaction ends `cancelled` (outcome void) | Funding not complete before the funding deadline | Stakes come back to the vaults (refund). Narrate it as the timeout rule, or set `FUNDING_WINDOW_SECS=300` on a fresh db and re-take |
| `setup-demo-vaults` → 409 `dev_owner_tx_failed` | Test owner out of USDC | Faucet, then repeat (it only tops up what is missing) |
| Tick 1 proposes nothing | Fake market not set, or the 600 s cooldown after a previous take | Re-post `/dev/fake-market`; re-takes on the same db need `PROPOSAL_COOLDOWN_SECS=0`, or use a fresh db (numbers above assume one) |
| A second challenge opens | Another tick after settle with the market still moved | Pause the bull: `POST /agents/$BULL/status {"status":"paused"}` (stops discovery and new commits, never resolution) |
| Tick before `end_ts` shows nothing | By design: Arc challenges run on the wall clock and never resolve early | Wait; the interaction's "Ends" tile shows the time |
| Vault panel shows drift briefly | Between resolve and release | Refresh; both happen in the same tick |
| Arc down entirely | — | Record the simulated run (section "Simulated run that works today", same screens, labelled simulated), plus a cutaway of the rehearsal txs above on the explorer |
