# Fund the Arc testnet addresses (owner action)

Unblocks the Arc testnet deploy (step 1 in `reports/handoffs/2026-11-17-chain-vaults.md`).
Testnet only: faucet USDC has no value. Never send real funds to these addresses.

## 1. Request faucet USDC

1. Open https://faucet.circle.com
2. Network: **Arc Testnet**, token: **USDC**
3. Paste an address, request (there may be a captcha). Repeat per address.

| Priority | Address | Role |
|---|---|---|
| **1 (unblocks deploy)** | `0x599F37a9815e13319E2C3AA04b850876706c4C90` | Escrow resolver + deployer: gas for deploy, create, resolve |
| 2 | `0x84dDb2795697e6EE319fe9bb55b5D5c1c3B90eca` | Vault operator: gas for agent stakes |
| 3 | `0x1F0eb73C714Ae0BD1a97c1c8e0F34513073393Fb` | Test owner: funds test-agent vaults in automated runs |
| 4 | your MetaMask address | Your owner wallet (browser UI, later step) |

On Arc, USDC is also the gas token and a transaction costs fractions of a cent, so one request each is plenty.
Faucet amount and rate limits are unverified. If you can only fund one, fund #1; the agent can forward gas to the others.

These addresses are derived from the testnet-only seed in `backend/.env` (D-58, gitignored, this machine only).
Deleting that file loses them and anything sent to them.

## 2. Check it arrived

In Claude Code, type:

```
! curl -s -X POST https://rpc.testnet.arc.io -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":1,"method":"eth_getBalance","params":["0x599F37a9815e13319E2C3AA04b850876706c4C90","latest"]}'
```

Any `result` other than `"0x0"` means funded. Or look the address up on https://explorer.testnet.arc.io

## 3. Add Arc Testnet to MetaMask (needed for the browser UI)

Settings → Networks → Add network → Add a network manually:

- Network name: Arc Testnet
- RPC URL: `https://rpc.testnet.arc.io`
- Chain ID: `5042002`
- Currency symbol: USDC
- Block explorer: `https://explorer.testnet.arc.io`

Source for these values: `docs/research/sponsors.md` §1.

## 4. Tell the agent

Say "continue" (or "funded"). The agent deploys ChallengeEscrow + VaultFactory and runs one manual create → stake → resolve check.
