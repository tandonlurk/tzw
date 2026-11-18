# Overnight run — 2026-11-18, 00:30 → 08:00 EST

Driven by `scripts/overnight.sh`: each `claude -p` run does the next unchecked item, commits, updates this file, and exits. All state lives here and in git, so a run cut off by a usage limit loses at most the item in progress.

Branch: `overnight-1118` (from `main` a2bcdcc, which now holds PRs #1–#3 via #4). Push the branch to `origin` after each commit (`git push -u origin overnight-1118`). Never merge, force-push, or push any other branch.

## Owner authorizations (2026-11-18, 00:25)

- Push `overnight-1118` to origin: yes.
- Live Arc **testnet** transactions: yes, at most ~5 testnet USDC staked per live run. Never mainnet.
- Off limits: mainnet; merging; UI restyling (owner redesigns Wed night, keep web edits minimal and functional); changing D-65 or custody (D-57); Grok/ElevenLabs/QuickNode live work (no keys in `.env`).

## Rules for each run

1. Read `AGENTS.md` 9.4, this file, then only the docs the current item needs (`TODO.md` M3b, `reports/handoffs/2026-11-18-approach-firms.md`, `docs/decisions.md` tail).
2. Take the first item that is `[ ]` or `[~]`. Mark it `[~]` and commit that before long work. If an item is `[~]` from a previous run, check `git log` and the notes under it, then continue it.
3. Done means: implemented, tests run and passing, `TODO.md` line ticked, new decisions as D-66+ in `docs/decisions.md`, a 1–3 line note under the item here (commit hash, test counts). Then commit (conventional message, no AI attribution) and push.
4. If an item is blocked or fails twice, mark it `[!]` with the reason and move to the next one. Don't burn the night on one item.
5. Do one or two items per run, then exit, so each run stays small. Delegate bulk implementation to Sonnet subagents. Keep reports short.
6. When every item is `[x]` or `[!]`, write `reports/handoffs/2026-11-18-overnight.md`, fill in "Morning summary" below, and put the line `OVERNIGHT: DONE` at the very end of this file.
7. After 07:30 EST, don't start a new item. Write the handoff and the summary, then add `OVERNIGHT: DONE`.

## Items

- [x] 0. Preflight: `cargo test --workspace`, `cd web && npm test && npm run build`, `cd contracts && forge test`. Record baseline counts here. If something fails on a clean `main`, that is item 0's fix.
  - Baseline on 83f2658: `cargo test --workspace` 388 passed, 1 ignored; `npm test` 112 passed (16 files), `npm run build` OK; `forge test` 100 passed. Nothing to fix.
- [x] 1. Fake market catalog: several fake markets (incl. a commodities/energy one, a non-crypto prediction market) so the market wheel demos offline with `MARKET_DATA=fake`. Tests. (TODO M3b, P1)
  - BTC + 6 fixed events / 14 contracts (ETH, WTI + natural gas = Commodities/Energy, gold, Fed, sports), D-66. +2 unit tests, preview/categories integration test rewritten; `cargo test --workspace` 390 passed, 1 ignored. HTTP check on a fresh db: categories/preview serve the catalog, simulated demo still +2.50/−2.50. Commit 87a974a.
- [x] 2. Testnet gaps: record the vault release tx and its activity row; `/stats/portfolio` for `arc_testnet`; discovery threshold from bid/ask mid instead of a stale last trade. Tests on anvil first, then one live Arc run (≤5 USDC) per `backend/README.md` "Run against Arc testnet"; record tx hashes and drift. (TODO M3b, P1)
  - Code 4dd6a8f (D-67): release txs + `vault_released` rows, `/stats/portfolio?env=`, dashboard testnet panel, bid/ask mid. `cargo test --workspace` 391 passed, 1 ignored (anvil chain tests ran); web 112 passed, build OK.
  - Live Arc run (1.25/side, ~0.088 USDC gas, funds withdrawn): settled, both release txs recorded, drift 0, testnet portfolio correct. Gemini PM API returned 500 all run, so discovery fell back to spot and the mid is NOT verified live; spot `last` was stale 17 min (tie → skeptic). Details + tx hashes: `reports/handoffs/2026-11-18-testnet-gaps.md`.
- [x] 3. Firm demo seed endpoint (`/api/dev/...`, dev-only like `seed-demo`) + README "Firm demo" section, using the script in `reports/handoffs/2026-11-18-approach-firms.md`. Integration test. (TODO M3b, P2)
  - `POST /api/dev/seed-firm-demo` (`demo_firm.rs`; firm create/join extracted to `create_in_tx`/`join_in_tx`), README "Firm demo", api.md row, D-68. +1 integration test walking the whole script over HTTP; `cargo test --workspace` 392 passed, 1 ignored; clippy clean. Real-server E2E (port 8091, fresh db `aen_firm_seed_e2e`): lead +3.80, scout 0.70, analyst 0.50, re-seed a no-op. Implemented by a Sonnet subagent, reviewed and re-tested by the lead.
- [x] 4. Starter templates with an approach node; rerun the simulated demo on a fresh db and update `docs/demo.md` numbers from the actual run. (TODO M3b, P2)
  - Migration 0012 (D-69): bull 15 %/100 %, skeptic 25 %/50 %, cautious 30 %/20 %, any market; firm seed + firm tests replace the template node. `cargo test --workspace` 392 passed, 1 ignored (templates test now checks the stances; Cautious approval test now 0.72); clippy clean. Commit 168bf87.
  - Fresh-db reruns over HTTP (`aen_item4_demo`, `aen_item4_firm`): BTC demo, co-captain, collaboration and firm demo numbers all unchanged; recorded in `docs/demo.md`. No web change needed (`ensureApproachNode` only adds when missing).
- [~] 5. Testnet demo script for recording the video: exact steps, expected screens, fallbacks if the RPC is slow. In `docs/demo.md` (testnet section). (TODO M6)
- [ ] 6. Full E2E regression on a fresh db: simulated demo end to end over HTTP + `npm run build`; browser walkthrough if Chrome tools are available, else say so. Findings → `reports/qa-overnight.md`; fix small P0/P1 bugs, list the rest.
- [ ] 7. Refresh `docs/submission/` with honest current status (what is live on Arc, what is simulated, what is unverified). (TODO M6)
- [ ] 8. Stretch: web app sends `AEN_API_TOKEN` when configured (SEC-02 gap); then `docs/design/explainers-storyboard.md`, a storyboard only (what to animate: discovery, negotiation, escrow lock, resolution, firm payout). No animation code.

## Notes

- Item 2: re-check the bid/ask mid live (read-only `/markets/preview`) if Gemini prediction markets answer again; item 6 or 7 can do it.

## Morning summary
