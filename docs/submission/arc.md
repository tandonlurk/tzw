# Best use of Arc (draft)

Criteria as published: chain award chosen by Circle; Arc testnet is fine.

## What is built and live on Arc testnet (chain id 5042002)

- `ChallengeEscrow` [0x4179…5698](https://explorer.testnet.arc.io/address/0x4179f2e314410D10C2906B1914bAAe8DD93b5698?tab=contract): two-party USDC escrow per challenge. A backend resolver key registers a challenge with explicit terms, both parties deposit before a funding deadline, the resolver pays the winner or voids, and anyone can trigger a refund after the resolve deadline. Deadline caps and "resolver cannot be a party" (SEC-04, SEC-07).
- `VaultFactory` [0x4EA6…4C45](https://explorer.testnet.arc.io/address/0x4EA6ac6A9F7b8fcf01ba04b177F0A098d9664C45?tab=contract) and one `AgentVault` per agent: the owner's wallet creates and funds the vault and can pause it or withdraw at any time; the backend's operator key may only stake into the escrow, within onchain per-challenge and exposure caps the owner set. All three are verified on Blockscout.
- USDC is the ERC-20 at `0x3600…0000` with 6 decimals, never `msg.value` (Arc pays gas in USDC).
- The server drives the whole lifecycle onchain (`backend/crates/server/src/lifecycle_arc.rs`): every stake still passes `core::authorize` first (and the owner's approval when it is above the agent's automatic limit); the ledger is credited only from verified receipts; ledger and vault balances are compared (`drift`) on every vault view.
- Tests: 100 Foundry tests (unit, fuzz incl. "operator cannot drain", invariants, adversarial; `tasks/contracts.md`), and server tests that run the real compiled contracts on anvil (full challenge, RPC outages, funding-deadline void, paused vault, sync idempotency).

## Live run with real Gemini data (2026-11-17/18)

The autonomous scheduler ran one full challenge on Arc testnet from a live Gemini prediction-market contract: discover, negotiate, owner approval, escrow create, both vault stakes, resolve onchain after the end time from Gemini's published outcome, ledger settle, vault release. Ledger = onchain for both vaults afterwards (drift 0). Commands and expected outcomes: `docs/demo.md`, "Testnet (Arc) demo"; full record: `reports/handoffs/2026-11-17-testnet-gemini.md`.

| Step | Tx |
|---|---|
| Bull vault create + link (test owner) | [0x4095be…99b7](https://explorer.testnet.arc.io/tx/0x4095beb06a825bc2bf9e89bcb9aec63b1c551cc8a0663e5a199203bd984099b7) |
| Bull vault fund 2.50 | [0x34466f…e953](https://explorer.testnet.arc.io/tx/0x34466fab84c2f803387127980cd590985afb1e5a092acc70c8294a6eff10e953) |
| Skeptic vault create + link | [0xe5bae7…b2bc](https://explorer.testnet.arc.io/tx/0xe5bae78e5c614375637aebe003822438b9345c80dc03829c1f5579e6e0adb2bc) |
| Skeptic vault fund 2.50 | [0x307baa…a5cf](https://explorer.testnet.arc.io/tx/0x307baa8bde05de28eb6cdf6a06b6519c0a30a8ab8ea89a6b0136733279f9a5cf) |
| Escrow `create` (resolver) | [0x9d139b…34de](https://explorer.testnet.arc.io/tx/0x9d139b98efceb996354fa2138b936eae061271b2c62a63bece1b821ee99834de) |
| Bull vault `stake` 1.25 (operator) | [0xa76c2c…141b](https://explorer.testnet.arc.io/tx/0xa76c2ccd8c9915cd4954df4445dedf69abe71c8de0c7f12e66203a5ccedf141b) |
| Skeptic vault `stake` 1.25 → Active | [0xfea2ff…28b1](https://explorer.testnet.arc.io/tx/0xfea2ffa3577c203f5796c03bedc4905bbb4654f8c9733668afad180e281928b1) |
| Escrow `resolve` → skeptic (Gemini: resolved no) | [0x602f22…a513](https://explorer.testnet.arc.io/tx/0x602f22fb4840d36f2b827fd6c2fa5b256d172c05593d00e2723892d72c91a513) |
| Bull vault `release` | [0x00a7f8…b0b2](https://explorer.testnet.arc.io/tx/0x00a7f851584d83727c9d2f7a3164dbb43cd84b91408ebe2956adfc99a963b0b2) |
| Skeptic vault `release` | [0xdb67e5…66e9](https://explorer.testnet.arc.io/tx/0xdb67e500322924d36ee109072d071558be3591fa46539b50d887a921a5e866e9) |
| Bull vault owner `withdraw` 1.25 | [0x4ac038…525b](https://explorer.testnet.arc.io/tx/0x4ac038d73e21f5682c24ea6dac4b937a19b92474aed6037ac1a2a629a7d0525b) |
| Skeptic vault owner `withdraw` 3.75 | [0x1c1955…99ad](https://explorer.testnet.arc.io/tx/0x1c195540d9ae532ab092e3ef7efd6a21a8ba753a68602f9a84a2fc3d5d0199ad) |

Vaults: bull [0xC287…8F11](https://explorer.testnet.arc.io/address/0xC287CA5b10CAE2F6486F5B9E355f9bf3Df848F11), skeptic [0xD6C9…9Ee4](https://explorer.testnet.arc.io/address/0xD6C940Af8282584b2f36cCdd8FbE7eCE03E09Ee4). Market: `GEMI-BTC15M2611180000-UP`; claim "YES price at or above 0.14 at 00:01:54 UTC", 1.25 per side after one counter-offer and one owner approval; Gemini resolved the contract **no**, so the skeptic won. Ledger = onchain after settlement (bull 1.25, skeptic 3.75, exposure 0) and after the withdrawals (0 / 0).

## Further live runs (2026-11-18, after release txs were recorded)

Both runs: same contracts, two fresh vaults funded 2.50 each by the test owner, 1.25 staked per side after one counter-offer and one owner approval, every vault `release` now stored and listed (with explorer links) on the challenge page, `/stats/portfolio?env=arc_testnet` kept apart from simulated totals, ledger = onchain afterwards (drift 0), all 5.00 withdrawn back to the owner.

- **Gemini spot fallback** (Gemini PM API was returning HTTP 500): claim "BTCUSD spot below 64,321.41"; Gemini's `last` did not move for 17 min, so it resolved as a tie for the skeptic. Record: `reports/handoffs/2026-11-18-testnet-gaps.md`. Gas ~0.088 testnet USDC.
- **Video script rehearsal** on the fake feed (`docs/demo.md` "Testnet video script"): proposal to settled in 4 min, bull +1.25 / skeptic −1.25. Gas ~0.07 testnet USDC.

| Step | Spot-fallback run | Rehearsal run |
|---|---|---|
| Escrow `create` | [0x19643b…18a5](https://explorer.testnet.arc.io/tx/0x19643bae86291031a856b12b7ff36433c8960615fc4f464aa2edc6d9348d18a5) | [0x8c3865…c2b1](https://explorer.testnet.arc.io/tx/0x8c3865598eb5621178f09717e739f020615df80c0c076c4ac427c49749a4c2b1) |
| Bull `stake` | [0x6a8413…7cd1](https://explorer.testnet.arc.io/tx/0x6a8413f7bf63a7d75ef02a6abd8a307305d2a9255e64ac04da2cce16ac0f7cd1) | [0xf96343…55ae](https://explorer.testnet.arc.io/tx/0xf963437dda4a611127b2e4e6436312e834160ae892f193cc66cd3bec20d355ae) |
| Skeptic `stake` | [0x2b4f88…2557](https://explorer.testnet.arc.io/tx/0x2b4f880d69c5946edca1a2adbd829ef1040fc3a393bfc0026f47d31bd1e2a557) | [0xe98b86…72bb](https://explorer.testnet.arc.io/tx/0xe98b861d79ead2c5ed38db556cd397be245f724a8f3dc14ec03182a446be72bb) |
| `resolve` | [0x8f4f7c…1ddf](https://explorer.testnet.arc.io/tx/0x8f4f7c22108b0c53e55c999c09ece8b7f5fb809dd49872dee72b0b97b8d61ddf) → skeptic | [0xf5c622…24a3](https://explorer.testnet.arc.io/tx/0xf5c622f8075800e63628417202da335ec249f76b8a86b37b2af2cbe8275624a3) → bull |
| Bull vault `release` | [0xb83df8…66e7](https://explorer.testnet.arc.io/tx/0xb83df8d5e98bd7c70b462160a428b5fb7cc9e42bcc3c526f07f81fa223ea66e7) | [0xfaa1e8…5863](https://explorer.testnet.arc.io/tx/0xfaa1e822c34c1995564cf7b0ba226c9737d53f21ebf76580045c48e358625863) |
| Skeptic vault `release` | [0xdba6bc…080d](https://explorer.testnet.arc.io/tx/0xdba6bc0cfb08842a5dceb593dfee146de51e0db66ef7591e8c9ee256a87f080d) | [0xcce32e…7529](https://explorer.testnet.arc.io/tx/0xcce32eb5382a4c568d9c6926d7c21ffe069dacff5d7440ac19f61a0c97567529) |

An earlier live run on the fake market feed (same contracts) is in `reports/handoffs/2026-11-17-arc-live-run.md`; a contract-level smoke run (create/stake/resolve/release/withdraw and `resolveVoid`) in `reports/handoffs/2026-11-17-chain-smoke.md`.

## Not done

- The owner's browser-wallet path (MetaMask creating and funding a vault from the web app) is built but not yet exercised with a real signature; the runs above use a testnet-only "test owner" key standing in for it (dev endpoint, `AEN_DEV_CHAIN_OWNER=1`).
- QuickNode Streams is not yet subscribed to escrow or vault events; the server reads receipts and contract state over RPC.
- Not run on Arc's Foundry fork (`arc-forge`); standard Foundry only. Refund after deadline, cap-violation reverts and blocklisted payouts are covered on anvil, not exercised on Arc.

## Trust model (stated plainly)

The resolver is a backend-administered key that reads Gemini market data; a market data API is not an oracle. A resolver can award any active challenge; there is no dispute process. The escrow's timeout refund is the only path that does not need the resolver. The operator key can only move vault funds into the escrow for a registered challenge, within the owner's onchain caps; payouts go back to the vaults, and only the owner can withdraw.
