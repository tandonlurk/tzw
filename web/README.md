# web

Next.js (App Router, TypeScript) owner console. Talks only to the backend API in `docs/api.md`.

```
cp .env.example .env.local   # NEXT_PUBLIC_API_URL, default http://localhost:8080/api
npm install
npm run dev                  # http://localhost:3000
npm run build && npm run lint && npm test
```

- Backend with `AEN_API_TOKEN` (SEC-02): set `NEXT_PUBLIC_API_URL=/backend` and, server-only, `AEN_API_TOKEN` (+ `AEN_API_URL` if the backend is not on `localhost:8080`), then rebuild. The browser calls `/backend/...` on this server (`app/backend/[...path]/route.ts`, helpers in `lib/proxy.ts`), which adds the bearer token and forwards; it applies its own Host allowlist (loopback + `WEB_ALLOWED_HOSTS`) and same-origin Origin check (D-70). The token never reaches the browser bundle.
- **UI structure** (design notes: `docs/design/ui.md`, D-72). Two modes over one app: Basic pages at `/`, `/agents`, `/agents/:id/{logic,activity,edit}`, `/activity`; Advanced under `/advanced` (terminal, agents table, `/advanced/agents/:id/{performance,logic,code,activity,settings}`, history). Wallet, challenges, firms, approvals, co-captain, settings and agent creation are shared. Shell: `components/shell/` (sidebar, breadcrumbs, mode switch, icons); mode mapping `lib/mode.ts`. Styles: `app/globals.css` (tokens + base), `shell.css`, `ui.css`, `canvas.css`, `terminal.css`, `code.css`.
- Agent workspace frame (both modes): `components/agent/AgentWorkspace.tsx` (one agent fetch shared by the tabs via `useWorkspace()`), tabs in `components/agent/`.
- Logic canvas: `components/canvas/` (React Flow; block shapes/colours and param summaries in `lib/blockVisuals.ts`). Replaced the old in-form `GraphBuilder`.
- Terminal: `components/terminal/` (panels + grid), layout model/presets/persistence `lib/layouts.ts`. Interactive chart `components/charts/TimeChart.tsx`, series helpers `lib/series.ts`.
- Code workspace: `components/code/CodeWorkspace.tsx`, highlighter `lib/jsHighlight.ts`.
- `lib/api`: typed client + contract types, uniform `ApiError` (including `unreachable`).
- `lib/money.ts`: BigInt-only formatting/parsing of minor-unit money strings.
- `lib/graph.ts`: contract `Graph` <-> canvas conversion; node positions are UI-only (localStorage `agent-graph-ui:<agentId>`).
- `components/builder`: params form and the inline custom-code editor, reused by the canvas side panel; block catalog from `GET /graph/blocks`.
- `scripts/dev-mock-server.mjs`: throwaway mock of part of the API for UI checks (`npm run dev-mock`, port 8080 or `PORT`). Not used by the app. Seeds three agents for the vault UI; `MOCK_CHAIN=disabled|missing` turns `/chain` off, `MOCK_OWNER=<address>` sets the mock vault owner (use your wallet to see owner-only actions), `MOCK_SEED=0` starts empty.
- Wallet / vault UI (Arc testnet, D-57): `lib/chain.ts` (pure: ABIs, calldata, amounts, add-chain params, error mapping), `lib/wallet.ts` (EIP-1193 + viem), `lib/txlog.ts`, `lib/vault.ts`, components in `components/wallet/`. Uses `viem` only; chain addresses come from `GET /api/chain` at runtime.
