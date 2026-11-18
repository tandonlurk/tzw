# Handoff 2026-11-18 — overnight run (items 0–8)

Branch `overnight-1118` (from `main` a2bcdcc), pushed to origin. **Not merged**: review and merge it yourself.
Per-item notes and commit hashes: `tasks/overnight.md`. Decisions D-66..D-70 (all "default, review").

## Final state (01:45 EST)

- `cargo test --workspace` 392 passed, 1 ignored (baseline 388); `forge test` 100 passed (unchanged);
  web `npm test` 123 passed (baseline 112), `npm run build` OK.
- No servers left running; scratch dbs dropped where created by item 8.

## What landed

| Item | Commit | Summary |
|---|---|---|
| 1 | 87a974a | Fake market catalog: BTC + 6 events / 14 contracts (crypto, WTI + natural gas, gold, Fed, sports), D-66 |
| 2 | 4dd6a8f, fe6959f | Vault release txs + `vault_released` rows, `/stats/portfolio?env=`, testnet dashboard panel, discovery at bid/ask mid (D-67). Live Arc run settled, drift 0 |
| 3 | 0598b9f | `POST /api/dev/seed-firm-demo` + README "Firm demo" (D-68) |
| 4 | 168bf87 | Starter templates carry an approach node (migration 0012, D-69); demo numbers re-verified unchanged |
| 5 | beff434 | `docs/demo.md` "Testnet video script", rehearsed live on Arc (fake market) |
| 6 | e515b4d | Fresh-db E2E regression; fixed escrow tx card on the challenge page, fake-market "Resolved by" label, env label (`reports/qa-overnight.md`) |
| 7 | 64b9491 | `docs/submission/` refreshed with honest live / simulated / unverified status |
| 8 | b943857, 4fd0bfe | Web app works with a token-protected backend via a same-origin proxy (D-70); explainer storyboard `docs/design/explainers-storyboard.md` |

Arc testnet spend: two live runs (items 2 and 5), 2.50 per vault and 1.25 staked per side each, ~0.16 USDC gas total; all
vault funds withdrawn back to the test owner. Tx hashes: `reports/handoffs/2026-11-18-testnet-gaps.md`, `docs/demo.md`.

## Needs your decision

1. **Gemini prediction-markets API returned HTTP 500 all night** (05:44–06:32 UTC checks). Discovery fell back to
   BTC spot; the bid/ask mid (D-67) is unit-tested but **not verified live**. Re-check `/markets/preview` before recording.
2. **Stale spot `last`**: Gemini BTC `last` didn't move for 17+ min, and the item 2 live run resolved on it (a tie,
   skeptic won). Options: a spot mid formula, or void when `last` is older than N minutes. Changing resolution is your call.
   Until then the video script defaults to the fake-market path on Arc (labelled).
3. Review D-66..D-70. D-70: the web proxy is opt-in (`NEXT_PUBLIC_API_URL=/backend`); anyone who can reach the web
   UI can drive the API through it, same as the UI itself, so run `next start -H 127.0.0.1` when a token is set.
4. Storyboard open questions (end of `docs/design/explainers-storyboard.md`): replay vs illustrative mode, tooling,
   scene 3 length, firm numbers depend on D-65.
5. Still open from before: D-65 firm split reading, PRs/merges, MetaMask check, keys for Grok / QuickNode / ElevenLabs.

## Not done / known gaps

- No Grok, QuickNode or ElevenLabs live work (no keys, off limits tonight).
- Browser checks were headless Chrome renders (no click-through); the Chrome extension tools were unavailable.
- Firms remain simulated-only (no vault-to-vault payouts on Arc).
