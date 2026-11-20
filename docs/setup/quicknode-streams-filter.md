# QuickNode Streams filter (Arc testnet)

The live stream `tzw-arc-testnet-escrow` (Arc Testnet, dataset Logs, webhook, no compression) keeps only logs from `ChallengeEscrow` and `VaultFactory`. The backend (D-74) also decodes and confirms `AgentVault` logs (`Staked`, `Released`) when they arrive, so adding the vault addresses to the filter fills the "Released to …'s vault" rows of a challenge's escrow table. Owner action; the filter was not changed by the agent that wrote this.

## Addresses to keep

`GET /api/streams` returns `watch_addresses`: escrow, factory, then every vault linked to an agent (lowercase). With the backend's token:

```sh
curl -s -H "Authorization: Bearer $AEN_API_TOKEN" http://127.0.0.1:8080/api/streams | jq -r '.watch_addresses[]'
```

A vault is linked when the owner creates it in the web app (or with `setup-demo-vaults`). New vaults need the filter updated again; that is the limit of a static list.

## Filter code

Replace the address set in the stream's filter with the list above. Keep the existing `return` shape if it differs: the backend accepts `{ data: [log, ...] }`, `{ data: [[log, ...], ...] }` (per block) and a bare array.

```js
// Arc testnet: TZW escrow, vault factory and linked AgentVaults only.
const WATCH = new Set([
  "0x4179f2e314410d10c2906b1914baae8dd93b5698", // ChallengeEscrow
  "0x4ea6ac6a9f7b8fcf01ba04b177f0a098d9664c45", // VaultFactory
  // AgentVaults from GET /api/streams -> watch_addresses, e.g.
  // "0xf78499779bcc48f4e96d34a06d6ae4869cc5b37f",
]);

function main(stream) {
  const data = stream.data ? stream.data : stream;
  const logs = [];
  const visit = (x) => {
    if (Array.isArray(x)) return x.forEach(visit);
    if (x && typeof x.address === "string" && WATCH.has(x.address.toLowerCase())) logs.push(x);
  };
  visit(data);
  return logs.length ? { data: logs } : null; // null: nothing is delivered for this batch
}
```

Test it in the Filter Editor against a block that holds one of the transactions from `backend/crates/server/tests/fixtures/quicknode/` (e.g. block 73112834, a `Deposited`) before saving.

## Not done

- No automatic vault discovery: QuickNode filters are stateless per batch as far as the docs we read say, so a vault created after the filter was saved is not watched until its address is added. Matching a `VaultCreated` in the same filter and then following that vault would need QuickNode's key-value store (not verified, not used).
