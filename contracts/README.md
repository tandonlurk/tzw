# contracts

`ChallengeEscrow.sol`: escrow for two-party, equal-stake challenges in one ERC-20 (USDC, 6 decimals).
Foundry project, forge-std only, no other dependencies.

**Status: tested locally only. Not deployed anywhere. Testnet funds only; mainnet use needs owner approval.**

## Design

| Function | Who | When | Effect |
|---|---|---|---|
| `create(id, partyA, partyB, stake, fundDeadline, resolveDeadline)` | resolver | id unused | Registers the challenge. Moves no funds |
| `deposit(id)` | a party, once | `Created`, `now < fundDeadline` | Pulls exactly `stake` via `transferFrom`. Second deposit makes it `Active` |
| `resolve(id, winner)` | resolver | `Active` (any time) | Pays `2 * stake` to `winner` (must be a party) |
| `resolveVoid(id)` | resolver | `Created` or `Active` | Returns each deposit to its depositor (draw, unresolvable, or cancel) |
| `refund(id)` | anyone | `Created` and `now >= fundDeadline`, or `Active` and `now >= resolveDeadline + GRACE` | Returns each deposit to its depositor |
| `claim(id, party)` | anyone | `owed[id][party] > 0` | Retries a payout that could not be pushed. Funds go to `party` only |
| `getChallenge(id)` | view | | Full stored state; `status == None` means unused |

`GRACE` is a constant, 1 hour. `create` requires `fundDeadline` within `MAX_FUND_WINDOW` (7 days) of now and `resolveDeadline` within `MAX_RESOLVE_WINDOW` (30 days) after `fundDeadline` (else `InvalidDeadlines`), and neither party may be the resolver (`InvalidParties`); so the timeout refund opens at most 37 days + 1 hour after `create`. `USDC` and `RESOLVER` are immutable. `totalHeld` is what the contract owes
(locked stakes plus deferred payouts).

```
                 create
   None ───────────────────────▶ Created ──── both deposited ────▶ Active
                                    │                                │
            resolveVoid (resolver)  ├──▶ Voided ◀── resolveVoid ─────┤
     refund (anyone, ≥ fundDeadline)├──▶ Refunded ◀── refund ────────┤ (anyone, ≥ resolveDeadline + GRACE)
                                                     Resolved ◀──────┘ resolve (resolver)
```

`Resolved`, `Voided` and `Refunded` are terminal. An id can never be reused.

Events (all with indexed `id`, parties indexed): `Created`, `Deposited`, `Activated`, `Resolved`, `Voided`,
`Refunded`, plus `PayoutDeferred` and `Claimed`.

### Choices where the spec left room

- **Only the resolver can `create`.** Nobody can register challenges naming other people. A party still
  consents by depositing its own stake itself; there is no deposit-on-behalf.
- **`resolve` is allowed at any time while `Active`**, before or after `resolveDeadline`, until someone uses
  the timeout `refund`. The contract cannot see the outcome, so an onchain "not before" check would add no
  safety against a wrong resolver; the backend must not resolve before the proposition's end time.
- **`resolveVoid` also works before funding completes**, so the backend can cancel without waiting for
  `fundDeadline`. It can only return each party's own deposit.
- **Deferred payouts.** USDC has a blocklist and on Arc transfers to a blocklisted address revert. A payout
  that fails is recorded in `owed` and emits `PayoutDeferred` instead of reverting, so one party's failing
  transfer cannot trap the other party's funds or block settlement. The indexer must treat `PayoutDeferred`
  as "settled, not yet paid" and `Claimed` as the payment.
- **No resolver transfer.** The resolver is fixed at construction. Rotating it means deploying a new escrow.
- **Deposits must arrive exactly.** `deposit` reverts unless the contract's balance rises by exactly `stake`,
  so fee-on-transfer, false-returning and no-op tokens cannot corrupt accounting. Only standard USDC is supported.
- No fees, no admin withdrawal, no upgradeability, no pause. No `payable`, `receive`, `fallback` or `msg.value`:
  on Arc USDC is also the gas token (18 decimals natively), and this contract only uses the 6-decimal ERC-20 interface.

## Trust assumptions

- **Resolution is backend-administered, not trustless.** The resolver (a backend key) alone decides the
  winner, from the Gemini exchange price it reads offchain. A compromised or wrong resolver can award any
  Active challenge to either party, or void it. There is no dispute process in the MVP.
- What the resolver cannot do: send funds to anyone other than a challenge's two parties, take a fee,
  touch another challenge's funds, or stop the timeout refund.
- If the resolver disappears, anyone can refund both stakes once `resolveDeadline + GRACE` passes.
  A resolver can still front-run that refund with a `resolve`.
- The token is trusted. Arc USDC is an upgradeable proxy controlled by Circle and can blocklist addresses.
  If the escrow address itself were blocklisted, all funds in it would be stuck.
- `block.timestamp` has 1-second granularity on Arc and several blocks can share a timestamp. Deadlines are
  only meaningful at a scale of minutes or more.

## Test

```
export PATH="$HOME/.foundry/bin:$PATH"
git submodule update --init --recursive   # forge-std
cd contracts
forge test
```

Unit, fuzz and invariant tests run against mock 6-decimal tokens (standard, blocklisting, fee-on-transfer,
false-returning, no-op, reentrant). The invariants: the token balance always equals locked stakes plus
deferred payouts and equals `totalHeld`; total paid out never exceeds total deposited.

These run on stock Foundry. They do not reproduce Arc-specific behaviour (native-USDC precompile, blocklist
semantics, gas rules). That needs Arc's Foundry fork (`arc-forge test --network arc`), which has not been run.

## Deploy to Arc testnet (UNTESTED)

Nothing below has been executed. Facts are from `docs/research/sponsors.md` section 1; verify before relying on them.

1. Install Arc's Foundry fork (`arc-forge`, `arc-cast`). Install source is still unverified.
2. Import a **testnet-only** deployer key into a keystore; never put it in a file or env var:
   `arc-cast wallet import arc-deployer --interactive`
3. Fund the deployer at https://faucet.circle.com (Arc Testnet). USDC is the gas token.
4. `cp .env.example .env`, set `RESOLVER_ADDRESS` (an address, not a key), then `source .env`.
5. Run the tests under Arc's fork first: `arc-forge test --network arc`
6. Deploy (chain ID 5042002; gas price must be at least 20 gwei or the tx is silently dropped):

   ```
   arc-forge script script/Deploy.s.sol:Deploy --rpc-url $ARC_TESTNET_RPC_URL \
     --account arc-deployer --with-gas-price 20gwei --broadcast
   ```

   The script refuses any chain other than Arc testnet (5042002) or a local chain (31337), and refuses a
   token that does not report 6 decimals.
7. Verify on Blockscout:

   ```
   arc-forge verify-contract $ESCROW src/ChallengeEscrow.sol:ChallengeEscrow --chain-id 5042002 \
     --verifier blockscout --verifier-url https://explorer.testnet.arc.io/api/ \
     --constructor-args $(arc-cast abi-encode "constructor(address,address)" $USDC_ADDRESS $RESOLVER_ADDRESS)
   ```

Open questions for a real deployment: required `evm_version` (set to `prague` here, unverified), whether stock
`forge` can deploy to Arc, and whether a `transfer` to a blocklisted address reverts or returns false on Arc
(the contract handles both).
