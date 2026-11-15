# web

Next.js (App Router, TypeScript) owner console. Talks only to the backend API in `docs/api.md`.

```
cp .env.example .env.local   # NEXT_PUBLIC_API_URL, default http://localhost:8080/api
npm install
npm run dev                  # http://localhost:3000
npm run build && npm run lint && npm test
```

- `lib/api`: typed client + contract types, uniform `ApiError` (including `unreachable`).
- `lib/money.ts`: BigInt-only formatting/parsing of minor-unit money strings.
- `lib/graph.ts`: contract `Graph` <-> canvas conversion; node positions are UI-only (localStorage `agent-graph-ui:<agentId>`).
- `components/builder`: React Flow builder; block palette and param forms come from `GET /graph/blocks`.
- `scripts/dev-mock-server.mjs`: throwaway mock of part of the API for UI checks (`npm run dev-mock`, port 8080). Not used by the app.
