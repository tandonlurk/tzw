# QA — overnight E2E regression (2026-11-18, item 6)

Branch `overnight-1118` after item 5 (fd361d6). Simulated funds only; one read-only look at the item 5 Arc testnet db (no transactions sent).

## What ran

| Check | Result |
|---|---|
| `backend/README.md` "Demo script", co-captain rounds, collaboration, verbatim over HTTP on a fresh db (`aen_item6`, port 8096, `DEV_ENDPOINTS=true MARKET_DATA=fake TICK_INTERVAL_SECS=0`) | Every number matches `docs/demo.md`: tick 1 proposed 1 / countered 1; tick 2 accepted 1 / approvals 1 / funded 0; early clock jump 409 `clock_jump_would_expire`; approve → `active`; resolve → settled, outcome `a`, observed 7000; bull +2.50 / skeptic −2.50; rounds 2–3 `{"settled":1,"suggestions":0}` then `{"suggestions":2}`; `lower_max_stake` accepted → skeptic v2, max 5.00; bull +7.50 3/3, skeptic −7.50 3/0; collaboration `["fund","cancel"]` → `approval_required` → `funded` → `+1.80/−1.80`; balances 109.30 / 90.70 |
| SEC-01 guard | `text/plain` approve → 415 `unsupported_media_type` |
| `npm test` | 114 passed (16 files), was 112 |
| `npm run build`, `tsc --noEmit`, `eslint` | OK; 1 pre-existing lint warning (`lib/graph.test.ts` unused `ui`) |
| Browser walkthrough | Claude-in-Chrome tools were **not available** in this unattended session. Used headless Chrome instead (`--dump-dom` after 8 s of virtual time, plus one `--screenshot`) against `next start` on :3000 with the backend above. All 14 routes rendered with live data and no error states: Dashboard, My Agents, agent detail, agent edit, new agent (3 templates), Approvals, Co-captain (both suggestions, loosens-control warning), Challenges list + detail, Collaborations list + detail, Firms, Wallet (simulated + "Arc testnet not enabled" panel), Activity. No clicks or form submissions were exercised in the browser; those paths were covered over HTTP. |

## Findings

| # | Sev | Finding | Status |
|---|---|---|---|
| Q-1 | P1 | Challenge detail did not show the escrow tx hashes the API already returns (`escrow.create_tx` … `counterparty_release_tx`); they were only in Activity details and ledger rows (item 5 note). | **Fixed**: "Onchain escrow (Arc testnet)" card, six steps in lifecycle order, explorer links when the backend's chain config is enabled (same helper as the ledger table), "not yet" for steps not reached. Checked by screenshot against the item 5 Arc db (`aen_item5_arc`): all six hashes shown. Links not exercised because that server ran without Arc config. |
| Q-2 | P1 | On the fake market the Terms row read "Resolved by: Gemini prediction markets data", which is untrue (the fake feed resolves it; terms keep the Gemini source name). | **Fixed**: reads "Offline fake market (stands in for Gemini prediction markets data), backend-administered (not trustless)" when `market_source == "fake"`. Live Gemini challenges unchanged. |
| Q-3 | P3 | Terms "Environment" showed the raw `arc_testnet`. | **Fixed**: "Arc testnet USDC, not real funds". |
| Q-4 | P3 | Dashboard "Realized P&L 0.00" for the portfolio while the two agents are +7.50 / −7.50. Correct (both agents are the owner's; the per-agent chart shows the split) but may confuse a video viewer. | Not changed; mention it in the voice-over. |
| Q-5 | P3 | Interaction header "round 2 of 3" means offers made, not demo rounds; reads oddly next to the co-captain "rounds". | Not changed (UI redesign is the owner's). |
| Q-6 | — | Pre-existing lint warning `lib/graph.test.ts:23` unused `ui`. | Not changed. |

No P0 found. No backend change.
