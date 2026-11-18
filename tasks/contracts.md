# Smart Contracts

- **Objective:** ChallengeEscrow on Arc testnet with tests and deploy docs.
- **Scope:** contracts/ only. See docs/architecture.md §7, decisions D-3, D-5, D-6, D-15.
- **Dependencies:** Owner approval of D-3 (custody) before any deploy; Arc facts in docs/research/sponsors.md; Foundry installed.
- **Assigned agent:** Smart Contracts specialist (branch `contracts`)
- **Status:** contract, tests, deploy script and README done. Local tests only. Not deployed.

## Files changed
- `.gitmodules`, `contracts/lib/forge-std` (git submodule, pinned to v1.17.0)
- `contracts/foundry.toml`, `contracts/.gitignore`, `contracts/.env.example`, `contracts/README.md`
- `contracts/src/ChallengeEscrow.sol`
- `contracts/script/Deploy.s.sol`
- `contracts/test/ChallengeEscrow.t.sol`, `ChallengeEscrow.invariant.t.sol`, `Deploy.t.sol`, `mocks/MockTokens.sol`

## Work completed
- Escrow: `create`, `deposit`, `resolve`, `resolveVoid`, `refund`, `claim`, `getChallenge`. Interface table and state diagram in `contracts/README.md`.
- Deploy script reads `USDC_ADDRESS` / `RESOLVER_ADDRESS` from env, key via keystore only, refuses chains other than Arc testnet and local.

## Tests run and results
- `forge test` (stock forge 1.9, solc 0.8.28, evm prague): 72 passed, 0 failed (69 unit/fuzz, 1 invariant suite with 4 invariants, 2 deploy script).
- Coverage of `ChallengeEscrow.sol`: 100% lines, 97.6% branches.
- Not run: `arc-forge test --network arc` (Arc fork not installed), any deployment.

## Decisions made (testnet defaults, for owner review; candidates for docs/decisions.md)
- `create` is resolver-only; parties consent by depositing themselves.
- `resolve` allowed any time while Active (before or after `resolveDeadline`) until the timeout refund is used.
- `resolveVoid` also works before funding completes (backend cancel).
- `GRACE` = 1 hour, constant.
- Failed payouts (USDC blocklist) are parked in `owed` and collected with `claim`, instead of reverting. Adds `PayoutDeferred` and `Claimed` events beyond the six in the brief.
- Resolver is immutable; no transfer.
- `deposit` requires the balance to rise by exactly `stake` (rejects fee-on-transfer / non-standard tokens).
- OpenZeppelin not used; a small inline reentrancy lock and low-level token calls were enough.

## Assumptions
- Stake token is standard USDC. Arc USDC at `0x3600…0000` has bytecode (checked with `eth_getCode` on the public testnet RPC).
- Deadlines are minutes or longer (Arc timestamps have 1s granularity).

## Blockers
- D-3 (custody) approval before deploying; Arc Foundry fork install source unverified.

## Unresolved questions
- Required `evm_version` on Arc; whether stock `forge` can deploy there.
- Exact failure mode of a transfer to a blocklisted address on Arc (revert vs false). Both are handled.

## Integration requirements
- After merging: `git submodule update --init --recursive` in each checkout.
- Backend indexer: handle `PayoutDeferred` (settled, unpaid) and `Claimed`; `Deposited` then `Activated` are emitted in the same tx on the second deposit.
- Backend must not call `resolve` before the proposition's end time; the contract does not enforce it.
- Each agent wallet must `approve` the escrow for `stake` before `deposit`.

## Recommended next steps
- Install `arc-forge`, run the suite with `--network arc`, then deploy to Arc testnet following `contracts/README.md`.
- Export the ABI (`contracts/out/ChallengeEscrow.sol/ChallengeEscrow.json`) for the backend.

## Log: SEC-04 / SEC-07 fixes (branch contracts-sec)
- `create` now reverts `InvalidDeadlines` if `fundDeadline - now > MAX_FUND_WINDOW` (7 days) or `resolveDeadline - fundDeadline > MAX_RESOLVE_WINDOW` (30 days). Refund is reachable at most 37 days + GRACE after create.
- SEC-07: `create` reverts `InvalidParties` if either party is the resolver.
- Tests: boundary, fuzz (refund reachable; out-of-bounds revert), old repro tests converted. 84 pass, `forge fmt` clean.
- ABI: no new errors/events/constructor change; two new constant getters `MAX_FUND_WINDOW()`, `MAX_RESOLVE_WINDOW()`. Backend must keep deadlines within the windows and never use the resolver address as an agent wallet.
- New defaults for docs/decisions.md: MAX_RESOLVE_WINDOW = 30 days (matches the report's suggestion; ample for demo/short-horizon challenges, bounds lock-up), MAX_FUND_WINDOW = 7 days (stops a far-future fundDeadline from bypassing the cap). Both are constants, easy to change before deploy.

## Log: AgentVault + VaultFactory (branch chain-vaults, 2026-11-16, D-57)
- `src/AgentVault.sol`: owner (browser wallet) funds, sets `maxPerChallenge`/`maxExposure`, pauses, replaces/revokes operator, withdraws any time. Operator may only `stake(id)` into an escrow challenge naming the vault, within caps; `release(id)` (anyone) frees exposure once the challenge is terminal. `VaultFactory.createVault(operator, caps, agentRef)` emits `VaultCreated`.
- `Deploy.s.sol` now deploys the factory too. `MockUSDC` emits `Transfer` (like real USDC).
- Tests: `forge test` 100 passed (16 new incl. fuzz: operator cannot drain).
- Not done: Arc testnet deploy (blocked on faucet funds, `docs/setup/arc-testnet-funding.md`).

## Log: Arc testnet smoke + verification (branch chain-smoke, 2026-11-17)
- `backend/crates/chain/examples/arc_smoke.rs`: live create → stake → resolve → release → withdraw, plus a `resolveVoid` path, via `aen-chain` on Arc testnet. Passed; 23 txs over two runs (first stopped by an RPC rate limit, resumed with `SMOKE_VAULT_A/B`), ~0.11 USDC gas total. Tx hashes: `reports/handoffs/2026-11-17-chain-smoke.md`.
- `aen-chain` fix: retry `-32005` rate limits (alloy `RetryBackoffLayer`) and poll receipts every 1 s on remote chains (was 7 s per tx). `cargo test -p aen-chain` passes.
- Blockscout: ChallengeEscrow, VaultFactory and AgentVault `0xa4CC…f90F` fully verified (0.8.28, opt 200, prague); other vaults match as a verified twin.
- Resolved questions: stock forge deploys on Arc; evm_version prague works. Resolver getter is `RESOLVER()` (`resolver()` reverts). Blocklist behaviour still untested on Arc.
