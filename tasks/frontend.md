# Frontend / UI

- **Objective:** Next.js app: dashboard, agents, creation, builder, wallet, activity.
- **Scope:** web/ only. No financial logic. Palette in AGENTS.md 4.4 as CSS variables.
- **Dependencies:** `docs/api.md` (v0).
- **Assigned agent:** Frontend / UI specialist (branch `frontend`)
- **Status:** M1 done against the contract, verified against the real backend (see reports/qa-m1.md)

## Files changed
`web/` (new). Key: `lib/api`, `lib/money.ts`, `lib/graph.ts`, `components/builder/*`, `components/AgentForm.tsx`, `app/**`, `scripts/dev-mock-server.mjs` (throwaway).

## Work completed
Shell + nav, typed API client, money utils, Dashboard, My Agents, Create (template -> form), Detail (allocate/deallocate, pause/resume, ledger), Edit (PUT = new version), Wallet (+ simulated deposit), Activity, visual builder (palette from `/graph/blocks`, params forms from `params_schema`, port type validation, `/graph/validate` errors on nodes, template load).

## Tests run and results
`npm run build`, `npm run lint`, `npm test` (15 unit tests: money, graph conversion) pass. Manually in browser: unreachable state, create flow with local validation, builder render + Validate, detail and dashboard against mock.

## Decisions made
- All data fetched client-side from `NEXT_PUBLIC_API_URL`.
- Dashboard groups totals by env so simulated and arc_testnet are never summed together.
- Node positions in localStorage keyed by agent id ("new" while creating, copied on create).

## Assumptions
- 422 `details` items look like `{field|path, message}` (strings also handled); unmatched ones show in a banner.
- Block port compatibility = equal `data_type`.

## Blockers
Real backend not run yet.

## Unresolved questions / change requests
- Contract could carry optional per-node `ui: {x, y}` so layout survives across devices.
- Document the 422 `details` shape.
- Template graphs/params must match `params_schema` types (integers as JSON numbers).

## Recommended next steps
Run against the real backend; charts (M2); approvals and interactions views.

## QA against real backend (qa-web)
- Positions now via node `ui {x,y}` (localStorage fallback). 422 graph paths mapped to nodes. Report: `reports/qa-m1.md`.
- Not browser-tested: add/connect/incompatible connection (unit-tested only).
