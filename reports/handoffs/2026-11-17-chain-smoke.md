# Handoff 2026-11-17 — Arc testnet smoke run + Blockscout verification (contracts/chain)

Branch `chain-smoke` (from `chain-vaults` 2f03581). Not pushed.

## What ran

`backend/crates/chain/examples/arc_smoke.rs`, using `aen-chain` (alloy) against the D-60 deployment.
Two full create → stake → resolve → release lifecycles and one `resolveVoid` path, all real txs on
Arc testnet (5042002). All onchain state reads matched expectations at every step.

Rerun (from `backend/`; reads `AEN_TESTNET_MNEMONIC` from env or from `AEN_ENV_FILE`, never prints it):

    AEN_ENV_FILE=/Users/landonturk/dev/hackathongs/tzw2/backend/.env cargo run -p aen-chain --example arc_smoke

Resume after an interruption (reuses vaults, tops up to 0.5, ends with withdraw):
`SMOKE_VAULT_A=0x.. SMOKE_VAULT_B=0x..`. Cost per fresh run ~0.08 USDC gas (two vault deploys are
~0.03 each); a resume run ~0.03. Vault funds go back to idx 2.

### Run 1 (stopped at step 7 by an RPC rate limit, see gotchas)

Vaults (owner idx 2, operator idx 1, caps 0.2 / 0.3 USDC): A `0xa4CC1095F398C13c643B2231696B9F348162f90F`, B `0xD206420e6732Eb32f9eB8c5a148009D641e2FD89`. Challenge `0x…e0adbd98b5034301be46c6656a8aa875`, stake 0.1, resolved to A.

| step | tx |
|---|---|
| createVault A | [0x9e0712…6ffc](https://explorer.testnet.arc.io/tx/0x9e071286cd5156a77a0e416ae2090696eda65cbebc872caf74c55f8410966ffc) |
| createVault B | [0xd9fd75…f53d](https://explorer.testnet.arc.io/tx/0xd9fd75a9e1a44303b8ae8d703dac53cebfed5af82a2a3945b4e0048bad9cf53d) |
| fund A 0.5 | [0x72ac22…68a8](https://explorer.testnet.arc.io/tx/0x72ac2281abbaf3b2d7cda04cefdf2579bfca49136385c012944d32b45f7468a8) |
| fund B 0.5 | [0x8ed3a4…4283](https://explorer.testnet.arc.io/tx/0x8ed3a4b36620bcfccc0bdaef05dab84a4fa707c6413b59dfd5ab1315c5084283) |
| escrow.create | [0x56e96e…4ac7](https://explorer.testnet.arc.io/tx/0x56e96e9a2e735ddd1f9d9172b4d8d56dea48e522b263d1f02f9913b90d884ac7) |
| vault A stake | [0x9d36a8…6d81](https://explorer.testnet.arc.io/tx/0x9d36a86f6956ed388a9ebf465fbb96fbc76a0987917be5b5d80251bacccd6d81) |
| vault B stake (→ Active) | [0x472de0…52b2](https://explorer.testnet.arc.io/tx/0x472de073016972cd12efe0127a9b135f0dd943b0a468d95ce7bf18f0d6ce52b2) |
| escrow.resolve → A | [0x2b5ee9…c9b6](https://explorer.testnet.arc.io/tx/0x2b5ee99e9069a347f2f2ee96dabf442c90648cb52456a670d4bd3548ea75c9b6) |
| vault A release | [0x9f8a67…4844](https://explorer.testnet.arc.io/tx/0x9f8a67152eb0e0f932c7934b8e81f7661c04d3444798eb9ae52a659187f24844) |
| vault B release | [0x8430ff…2447](https://explorer.testnet.arc.io/tx/0x8430ffbbc1f18f7fe95f77f11d7d114cdae455547025cedb12fb8e0f64242447) |

### Run 2 (resume with the same vaults, completed: 13 txs in 27 s)

Challenge #1 `0x…1fef8d533c7e4c29b93b613b99608030` (A vs B, resolved to A); challenge #2 `0x…5c74a97a21714b2c8ecc2b182f950d14` (only A staked, voided, A refunded).

| step | tx |
|---|---|
| fund B top-up 0.1 | [0x760323…abdc](https://explorer.testnet.arc.io/tx/0x7603236ade5c178e8292b06a33652e07b7be24cd7b4bd6d2fc0c10ca4b08abdc) |
| escrow.create #1 | [0x68eb0a…ef0d](https://explorer.testnet.arc.io/tx/0x68eb0ab56b99a45c95beca01b6a64751509c3cafa222df39fb33bcebf7aeef0d) |
| vault A stake #1 | [0x4efb37…87bf](https://explorer.testnet.arc.io/tx/0x4efb37f861966c8962743baa6eb5e44737243585d09537c8314d1551beef87bf) |
| vault B stake #1 (→ Active) | [0x743695…988a](https://explorer.testnet.arc.io/tx/0x743695573e0a17c65711509b9f792d1ef97b26559c7a597bf60f8327f438988a) |
| escrow.resolve #1 → A | [0x5d179f…c86e](https://explorer.testnet.arc.io/tx/0x5d179f31bcd060df025cd25d869bc3786a657bafa7ef33771ff19bf41f89c86e) |
| vault A release #1 | [0x356efe…f3e4](https://explorer.testnet.arc.io/tx/0x356efeacaf5702a01f1c38e8222bfc631213b838b2124479dc09bac87ca9f3e4) |
| vault B release #1 | [0x9c26b2…b856](https://explorer.testnet.arc.io/tx/0x9c26b24f2078b9bf41d451c274207d43f1eb18c7bf04df7759fcacd9c0e4b856) |
| escrow.create #2 | [0x171e80…94e5](https://explorer.testnet.arc.io/tx/0x171e80e01b7a5d4c633e14333f1466bdb8cbb0e8b532d3374b31e5510fca94e5) |
| vault A stake #2 | [0x5fe809…4445](https://explorer.testnet.arc.io/tx/0x5fe809cb4cdce490696bac62ae4d6478e3c3f5409b921ee20fc3152b2b924445) |
| escrow.resolveVoid #2 | [0x46f73c…ef8d](https://explorer.testnet.arc.io/tx/0x46f73c7b15a41e7e564384e2b4bbcb7f81a9ebd9a34d9aa666679b3a6a9fef8d) |
| vault A release #2 | [0x096ea9…159d](https://explorer.testnet.arc.io/tx/0x096ea96bb4e66214040e3e749f5843b03f55de646581325747615dab7059159d) |
| vault A withdraw 0.7 | [0xc1b340…48d2](https://explorer.testnet.arc.io/tx/0xc1b3404b979da6274eb5f38d7f9e11ec5c9413380031d46e69308279eebb48d2) |
| vault B withdraw 0.4 | [0xc3c113…2173](https://explorer.testnet.arc.io/tx/0xc3c1131ec5ebadd4fd1c03006a78b7f909e62022ad1dfbbb8ed53f5a10a52173) |

Both vaults end empty with zero exposure. Total spend over both runs ~0.11 USDC (balances after:
idx 0 19.933477, idx 1 19.973404, idx 2 19.931363).

## Verification (Blockscout)

All fully verified (solc v0.8.28+commit.7893614a, optimizer 200, evm prague):
- ChallengeEscrow https://explorer.testnet.arc.io/address/0x4179f2e314410D10C2906B1914bAAe8DD93b5698?tab=contract
- VaultFactory https://explorer.testnet.arc.io/address/0x4EA6ac6A9F7b8fcf01ba04b177F0A098d9664C45?tab=contract
- AgentVault (vault A) https://explorer.testnet.arc.io/address/0xa4CC1095F398C13c643B2231696B9F348162f90F?tab=contract — other vaults show as a verified twin (vault B already does).

How: `forge verify-contract <addr> <src:Name> --verifier blockscout --verifier-url https://explorer.testnet.arc.io/api/ --compiler-version 0.8.28 --evm-version prague --optimizer-runs 200 --constructor-args $(cast abi-encode ...) --chain 5042002`. Constructor args: escrow `(USDC, resolver idx 0)`, factory `(USDC, escrow)`, vault `(USDC, escrow, owner, operator, maxPerChallenge, maxExposure)`. For the vault, forge hit Blockscout's API rate limit ("Too many requests"), so it was submitted with the v2 endpoint `POST /api/v2/smart-contracts/<addr>/verification/via/standard-input` (standard JSON from `forge verify-contract --show-standard-json-input`).

## Arc vs anvil — gotchas for the backend wiring

1. **Resolver getter is `RESOLVER()`** (immutable), not `resolver()`. `cast call 0x4179…5698 "RESOLVER()(address)" --rpc-url https://rpc.testnet.arc.io` → `0x599F37a9815e13319E2C3AA04b850876706c4C90`; `resolver()` reverts (no such selector). That explains the earlier "resolver() read reverted". `Chain::connect` already uses `RESOLVER()`.
2. **Public RPC rate limit**: `https://rpc.testnet.arc.io` returned `-32005 rate limit exceeded` mid-run (run 1). Fixed in `aen-chain` `signing_provider`: alloy `RetryBackoffLayer(10 retries, 1 s backoff, 100 CU/s)`; `-32005` is retryable in alloy. A server tick that does several reads + txs per interaction will hit this without the retry; still handle `ChainError::Rpc` as "retry next tick".
3. **Receipt latency**: alloy polls remote endpoints every 7 s, so each tx took ~7 s. Now 1 s for non-local URLs; observed ~1.1–1.5 s per confirmed tx (Arc finality is sub-second).
4. **Gas**: EIP-1559 (type 2) works with alloy's default estimation; no explicit gas price needed in `aen-chain` (the forge deploy was run with `--with-gas-price 20gwei`). Base fee floor 20 gwei; effective price observed 22–32 gwei. Gas used: createVault ~967k (~0.03 USDC), stake ~166k, create ~97k, resolve ~92k, resolveVoid ~70k, withdraw ~52k, release ~38k, transfer ~49k. Budget ~0.005 USDC per challenge for the resolver and ~0.01 for the operator.
5. **Decimals**: USDC ERC-20 at `0x3600…0000` reports 6 decimals; native balance (`eth_getBalance`, gas) is the same USDC with 18 decimals. Always use the ERC-20 view for ledger amounts.
6. **Resolve has no time check onchain**: the resolver can resolve as soon as the challenge is Active (the backend must wait for `end_ts`, as documented). `stake` only works while `Created` and before `fundDeadline`.
7. **Blocklist**: not exercised (no blocklisted address available). Payouts to the two vaults went straight through; no `PayoutDeferred`.
8. evm_version prague and stock forge 1.9 bytecode run fine on Arc; nothing Arc-specific needed in the contracts.

## Files changed

- `backend/crates/chain/examples/arc_smoke.rs` (new)
- `backend/crates/chain/src/lib.rs` (`signing_provider`: retry layer + 1 s poll interval; no API change; `cargo test -p aen-chain` passes)
- `reports/handoffs/2026-11-17-chain-smoke.md`, `tasks/contracts.md`

## Proposed decision text (lead to consolidate into docs/decisions.md)

- D-6x | 2026-11-17 | default, review | Arc testnet smoke run passed with `aen-chain` (create/stake/resolve/release/withdraw + resolveVoid, tx hashes in `reports/handoffs/2026-11-17-chain-smoke.md`). ChallengeEscrow, VaultFactory and one AgentVault verified on Blockscout. `aen-chain` retries rate-limited RPC calls (`-32005`) and polls receipts every 1 s on remote chains | The public Arc RPC rate-limits bursts; alloy's 7 s default poll made each tx 7 s.

## Remaining

- Not tested on Arc: `refund` after deadlines (needs waiting ≥ fundDeadline; the anvil test covers it), cap-violation reverts (`OverPerChallenge`/`OverExposure`, covered on anvil), blocklisted payouts.
- Server wiring (handoff 2026-11-17-chain-vaults "Next steps" 2–5) is unchanged and still to do.
