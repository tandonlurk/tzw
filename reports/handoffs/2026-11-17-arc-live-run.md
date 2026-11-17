# 2026-11-17 — first live Arc testnet lifecycle through the server (lead)

Merged into `chain-vaults`: `chain-smoke` (D-61), `wallet-ui` (335d62b, D-62), `chain-wiring` (258e722, D-63).
Merged-branch checks: `cargo test --workspace` 341 passed / 0 failed / 1 ignored; web 82 tests, lint 0 errors, build OK.

## Live run (real server binary, Arc testnet 5042002, `MARKET_DATA=fake`, fresh db `aen_arc_demo`)

seed-demo → `POST /dev/chain/setup-demo-vaults {"amount":"3000000"}` → bull `horizon_secs` 60 → fake market → 2 ticks
(proposed, countered, accepted, approval) → approve skeptic → 1 tick (escrow create + both stakes, `active`) →
fake market YES 0.70 → tick after `end_ts` → `settled` with an onchain resolve.

| Step | Tx |
|---|---|
| Bull vault link (createVault) | 0x61b122e70ee5e8eaf5f021bd1be2ef84a81f5d7e7048a16e69d4d4665d1e86ac |
| Bull vault fund 3 USDC | 0x33f0ff734bb1ab65d06574034a26d9385810d428db35dcb596a20191b7d82dc4 |
| Skeptic vault link | 0x3dd592d78a4c4cbc6a2c11510a42948cf22788f6c1938977fd927b10cfc2d986 |
| Skeptic vault fund 3 USDC | 0xd69f4cd142528db0b84e4f10ae4559fff130898e0d3c9e46cccde04865c746f7 |
| Escrow create | 0x3ff82f2dc5cb94d99ab8b4524546264e6c67ce3b228363f0f329e2c228ffd18d |
| Bull stake 1.50 | 0xb92420fa3af459ceadd242f47d6b51c6def6c1ca8d08641bea7e8963c4b2dddf |
| Skeptic stake 1.50 | 0x18990594a69b1acf5a1d702c4461dc115b983f3fdbc9b9b1559e670ba87206da |
| Resolve (bull wins) | 0x4beffa6e3791a08fe79d656793d26e394e4d28b0ec6e1df1981d1cc13e5cfeab |

Explorer: `https://explorer.testnet.arc.io/tx/<hash>`. Vaults: bull `0xD4AB39f77aAAFf0077d6E08cbAdB80E6EBfC5149`,
skeptic `0x9c8Df566eBAc0A73E6ECD7d7e2f10E76626Bb97b` (owner = test owner idx 2).

Result: bull onchain 4.50 = ledger 4.50, skeptic onchain 1.50 = ledger 1.50, exposure 0, drift 0 on both.

## Gaps found

- Tick report counters: the tick that created + staked the escrow reported `funded: 0` although the challenge went `active` (cosmetic).
- Market data was the fake feed; a run with real Gemini data is still to do.
- Wallet UI not yet exercised with a real MetaMask signature (owner action).
- Not done: Streams on escrow/vault events, Grok key, ElevenLabs.
