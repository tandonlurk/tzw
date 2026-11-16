# Best use of Arc (draft)

Criteria as published: chain award chosen by Circle; Arc testnet is fine.

## What is built

- `contracts/src/ChallengeEscrow.sol`: two-party USDC escrow for a challenge. Create with explicit terms, both parties deposit before a funding deadline, a designated resolver pays the winner or voids, and anyone can trigger a refund after the resolve deadline. Deadline caps (7 d funding, 30 d resolve) and "resolver cannot be a party" (SEC-04, SEC-07).
- 84 Foundry tests including fuzz and adversarial cases (`cd contracts && forge test`).
- USDC treated as an ERC-20 with 6 decimals, never `msg.value` (Arc uses USDC as gas; see `docs/research/sponsors.md` section 1).

## Not done

- **Not deployed to Arc testnet.** Deployment, the signer and the chain executor wait on the owner's custody decision (D-3) and faucet funds. Until then the same lifecycle runs on the simulated ledger.
- Not run on Arc's Foundry fork (`arc-forge`); standard Foundry only.

## Trust model (stated plainly)

The resolver is a backend-administered key. A resolver can award any active challenge; there is no dispute process. The timeout refund is the only path that does not need the resolver.

## If deployed before submission

Add: contract address and Blockscout link, the two deposit transactions and the settlement transaction from one demo run, and which steps used QuickNode Streams events.
