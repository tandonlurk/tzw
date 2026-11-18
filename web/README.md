# web

Next.js (App Router, TypeScript) owner console. Talks only to the backend API in `docs/api.md`.

```
cp .env.example .env.local   # NEXT_PUBLIC_API_URL, default http://localhost:8080/api
npm install
npm run dev                  # http://localhost:3000
npm run build && npm run lint && npm test
```

- Backend with `AEN_API_TOKEN` (SEC-02): set `NEXT_PUBLIC_API_URL=/backend` and, server-only, `AEN_API_TOKEN` (+ `AEN_API_URL` if the backend is not on `localhost:8080`), then rebuild. The browser calls `/backend/...` on this server (`app/backend/[...path]/route.ts`, helpers in `lib/proxy.ts`), which adds the bearer token and forwards; it applies its own Host allowlist (loopback + `WEB_ALLOWED_HOSTS`) and same-origin Origin check (D-70). The token never reaches the browser bundle.
- `lib/api`: typed client + contract types, uniform `ApiError` (including `unreachable`).
- `lib/money.ts`: BigInt-only formatting/parsing of minor-unit money strings.
- `lib/graph.ts`: contract `Graph` <-> canvas conversion; node positions are UI-only (localStorage `agent-graph-ui:<agentId>`).
- `components/builder`: React Flow builder; block palette and param forms come from `GET /graph/blocks`.
- `scripts/dev-mock-server.mjs`: throwaway mock of part of the API for UI checks (`npm run dev-mock`, port 8080 or `PORT`). Not used by the app. Seeds three agents for the vault UI; `MOCK_CHAIN=disabled|missing` turns `/chain` off, `MOCK_OWNER=<address>` sets the mock vault owner (use your wallet to see owner-only actions), `MOCK_SEED=0` starts empty.
- Wallet / vault UI (Arc testnet, D-57): `lib/chain.ts` (pure: ABIs, calldata, amounts, add-chain params, error mapping), `lib/wallet.ts` (EIP-1193 + viem), `lib/txlog.ts`, `lib/vault.ts`, components in `components/wallet/`. Uses `viem` only; chain addresses come from `GET /api/chain` at runtime.
