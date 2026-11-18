# 2026-11-17/18 — Arc testnet lifecycle on live Gemini data (testnet operator)

Branch `testnet-gemini` (from d46a107). Not pushed. No code changed.

## Outcome

Settled, drift 0. One challenge ran end to end with the **autonomous scheduler** (`TICK_INTERVAL_SECS=10`), real Arc testnet USDC in two agent vaults and **real Gemini prediction-market data** (`MARKET_DATA=gemini`). Manual steps were the owner's only: one config edit, one approval, a pause, and the final withdraw.

- Server: the branch's `aen-server` binary (built into the shared target, run from a copy), fresh db `aen_arc_gemini`, port 8080, `FUNDING_WINDOW_SECS=180`, Arc env per `backend/README.md` "Arc testnet", `AEN_DEV_CHAIN_OWNER=1 DEV_ENDPOINTS=true`.
- Market: `GEMI-BTC15M2611180000-UP` ("BTC price today at 7pm EST: Up", Gemini 15-minute up/down, reference 64,923.72 at 23:45 UTC, expiry 00:00 UTC), last trade 0.14 at discovery.
- Claim (bull, proposer): YES price at or above 0.14 at 00:01:54 UTC. Bull offered 2.50/side, skeptic countered 1.25, bull accepted; 1.25 is above the skeptic's 1.00 automatic limit → approval requested → owner approved.
- Real observation: Gemini settled the event at 00:00:08 UTC, `resolutionSide: "no"`, settlement 64,874.75. Backend read it at 00:02:02 (`observed 0`, detail "contract resolved no", source `gemini`) → outcome B, skeptic wins.
- After settle + release: bull vault onchain 1.25 = ledger 1.25, skeptic 3.75 = 3.75, exposure 0, drift 0. Agent stats: bull realized -1.25 (0/1), skeptic +1.25 (1/1).
- Owner withdrew both vaults (cast, test-owner key) and synced each tx: both 0 onchain = 0 ledger, drift 0.

Timeline (UTC): 23:47:30 proposed + countered (same tick) · 23:47:40 accepted, approval requested · 23:47:50 approved → `funded` · 23:48:01–05 escrow create + 2 stakes → `active` · 00:00:08 Gemini settles · 00:02:02 resolve tx, ledger settled · 00:02:03–04 both releases.

## Transactions (https://explorer.testnet.arc.io/tx/<hash>)

| Step | Signer | Tx |
|---|---|---|
| Bull vault create + link | owner idx 2 | 0x4095beb06a825bc2bf9e89bcb9aec63b1c551cc8a0663e5a199203bd984099b7 |
| Bull vault fund 2.50 | owner idx 2 | 0x34466fab84c2f803387127980cd590985afb1e5a092acc70c8294a6eff10e953 |
| Skeptic vault create + link | owner idx 2 | 0xe5bae78e5c614375637aebe003822438b9345c80dc03829c1f5579e6e0adb2bc |
| Skeptic vault fund 2.50 | owner idx 2 | 0x307baa8bde05de28eb6cdf6a06b6519c0a30a8ab8ea89a6b0136733279f9a5cf |
| Escrow create | resolver idx 0 | 0x9d139b98efceb996354fa2138b936eae061271b2c62a63bece1b821ee99834de |
| Bull stake 1.25 | operator idx 1 | 0xa76c2ccd8c9915cd4954df4445dedf69abe71c8de0c7f12e66203a5ccedf141b |
| Skeptic stake 1.25 (→ Active) | operator idx 1 | 0xfea2ffa3577c203f5796c03bedc4905bbb4654f8c9733668afad180e281928b1 |
| Resolve → skeptic vault | resolver idx 0 | 0x602f22fb4840d36f2b827fd6c2fa5b256d172c05593d00e2723892d72c91a513 |
| Bull vault release | operator idx 1 | 0x00a7f851584d83727c9d2f7a3164dbb43cd84b91408ebe2956adfc99a963b0b2 |
| Skeptic vault release | operator idx 1 | 0xdb67e500322924d36ee109072d071558be3591fa46539b50d887a921a5e866e9 |
| Bull vault withdraw 1.25 | owner idx 2 | 0x4ac038d73e21f5682c24ea6dac4b937a19b92474aed6037ac1a2a629a7d0525b |
| Skeptic vault withdraw 3.75 | owner idx 2 | 0x1c195540d9ae532ab092e3ef7efd6a21a8ba753a68602f9a84a2fc3d5d0199ad |

Vaults (owner idx 2, operator idx 1, caps 2.50/2.50): bull `0xC287CA5b10CAE2F6486F5B9E355f9bf3Df848F11`, skeptic `0xD6C940Af8282584b2f36cCdd8FbE7eCE03E09Ee4`. Interaction `041ed267-927a-44c1-8c36-618bb1cf0e45`.

## Balances (testnet USDC, ERC-20 view)

| Key | Before | After | Spent |
|---|---|---|---|
| idx 0 resolver `0x599F…4C90` | 19.928973 | 19.922831 | 0.006142 |
| idx 1 operator `0x84dD…0eca` | 19.963140 | 19.951464 | 0.011676 |
| idx 2 test owner `0x1f0e…93fb` | 13.865344 | 13.795894 | 0.069450 (2 vault creates ~0.06, transfers, withdraws) |

Total cost ~0.087 USDC in gas; the 5.00 funded into the vaults came back. Both vaults are empty.

## Config changes (owner edits through the API, no code)

Bull v2 → v3 via `PUT /agents/…d1`:
- `trigger.market_move.threshold_bps` 200 → 1 (minimum allowed; 0 is refused by the validator). Real BTC moved +0.22% over the hour; the seeded 2% trigger would not fire on a quiet market.
- `cond.conviction_gte.min_bps` 6000 → 5000 and `policy.min_conviction_bps` 6000 → 5000. The rule reasoner's conviction is `5000 + 5 × move_bps` (51% at +0.22%), so the 60% gate needs a ≥2% move. `policy.min_conviction_bps` is not enforced by `authorize`; changed only to keep the two consistent.
- `config.horizon_secs` 3600 → 685, chosen so `end_ts` (= proposal + 180 s funding window + horizon) lands ~1.5 min after the 00:00 expiry: Gemini settles BTC events ~6–9 s after expiry, so the challenge resolves on Gemini's published outcome rather than on a possibly stale last trade.
- Vault amount 2.50 (via `setup-demo-vaults {"amount":"2500000"}`) so the skeptic's counter (1.25) lands above its 1.00 limit and shows the approval step, within the 6 USDC budget.
- After activation the owner **paused** the bull, so it would not open a second challenge on the next 15-minute contract (pausing stops discovery and commits, not resolution).

## Findings

1. **`funded: 0` counter (not fixed; assigned to the backend agent as a new `activated` field).** `TickReport.funded` counts challenges whose two stakes were *reserved in the ledger* by the tick's commit phase (`commit_one` → `CommitOutcome::Funded`). When the owner's approval completes the reservation (`POST /approvals/{id}/approve` calls `try_commit` itself), no tick counts it; the next tick's `lifecycle_arc::fund_phase` creates the escrow, stakes both vaults and moves the challenge to `active` via `activate()`, which increments no counter. Same in this run.
2. **Vault release txs are not recorded.** `release_one` sets `vaults_released_at` but stores neither tx hash nor an activity row; I found them on the explorer under the operator address. Small, isolated follow-up (column or activity row in `lifecycle_arc.rs`).
3. **Dashboard is simulated-only.** `GET /stats/portfolio` hardcodes `Env::Simulated` (`interactions.rs`), so the testnet result does not appear in the dashboard totals/chart; per-agent `stats` and Activity do show it.
4. **Threshold from a stale last trade.** Discovery uses `lastTradePrice` (0.14) while the book was 0.34/0.41 at proposal time. Honest per the formula, but the claim was lopsided. Using the mid when both sides exist would be a discovery change (owned by the backend agent).
5. **Market choice.** Discovery takes the soonest-expiring event with a traded contract in 0.05–0.95, so it chose the 15-minute up/down event over the hourly ladder with the same expiry. Fine for a short demo.
6. Verified live: `resolutionSide: "no"` (previously "Not verified"); updated `docs/research/gemini-prediction-markets.md`.
7. No server warnings or errors during the run; no RPC rate-limit hits.

## Files changed

- `docs/demo.md` — new section "Testnet (Arc) demo" with commands, observed steps, caveats.
- `docs/submission/arc.md` — rewritten status: deployed + verified, live run tx table, not-done list, trust model.
- `docs/submission/gemini.md` — live run, last-trade caveat, `"no"` seen.
- `docs/submission/main-track.md`, `docs/submission/README.md` — status lines.
- `docs/research/gemini-prediction-markets.md` — `resolutionSide: "no"` seen, settle lag.
- this handoff.

## For the owner (evening review)

- Nothing new needs a decision. The 4 config edits above are on the demo db only.
- Still open: the MetaMask path (owner signs vault create/fund in the web app), Streams on escrow/vault events, Grok key, ElevenLabs.
