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

## M2 (frontend-m2)
- Added: typed M2 client/types, `/approvals` (approve/reject; approve returning cancelled shows a clear "authorization denied" notice), `/interactions` list + detail (terms, negotiation, approvals, decision trace, timeline, ledger), Activity with links + raw details, dashboard charts from `/stats/portfolio` only (SVG components in `components/charts`), agent detail challenges, labelled "Dev: run tick" (hidden on 404), template description prefill.
- Helpers with tests: `lib/interactions.ts`, `lib/charts.ts` (21 tests total).
- Browser-verified against real backend: approvals approve flow, dashboard numbers vs API, interaction detail, builder add block / incompatible connection rejected / compatible connection drawn.
- Not browser-verified: reject path, approve -> cancelled notice, agent detail challenges section, create-form prefill.

## M2b (web-m2b)
- Dashboard P&L chart: "By agent" lines (default, mint/rose/ice by final sign, from `/stats/portfolio` `pnl_over_time.agent_cumulative`) with a "Portfolio total" toggle. Helpers `perAgentSeries`/`scaleMulti` in `lib/charts.ts` (tests).
- Approval-cancelled notice no longer shows a doubled period (`reasonSuffix`, tested).
- "1 Issue" badge: React hydration warning, extension (Grammarly) adds `data-gr-ext-installed` / `data-new-gr-c-s-check-loaded` on `<body>`. Fixed with `suppressHydrationWarning` on body in `app/layout.tsx`.
- Browser-verified on real backend: reject, approve-then-cancelled notice, agent-detail challenges list, template description prefill, per-agent chart. Name is intentionally left blank on template pick.

## Co-captain suggestions view (web-cocaptain)
- Added `/cocaptain` page (one nav item), `lib/suggestions.ts` helpers + tests, API types/client for list/generate/accept/dismiss.
- Pending list with before->after table (current values from `GET /agents`, display only), amber loosens-control warning, evidence in `<details>`, 422/409 handling, "Check now", decided/expired list.
- Verified in browser against a real backend (fake market, seed-demo): list, accept (agent v2), dismiss, 422 stays pending, 409 stale. Suggestions with proposed changes were inserted via SQL (rules did not produce one in the short demo path).
- Not done: dashboard/agent-detail pending count.

## Agent detail + editing (web-m3, stage 1)
- Audit: detail already had balances, realized P&L, config, policy, challenges, ledger; edit (PUT = new version) with inline 422 field errors existed. Missing: unrealized P&L, win rate, active count, version history, per-agent activity, saved-version confirmation, version shown on decision traces.
- Added: unrealized P&L tile ("n/a, not computed"), win rate %, active challenges tile, Version history (`GET /agents/{id}/versions`, newest first, diff vs previous), Recent activity, "Saved as version N" notice after edit, "(config vN)" on run traces in interaction detail. Collaborations labelled not available. Helpers + tests in `lib/agentView.ts` (33 web tests at this commit).
- Not browser-verified this stage (build/lint/tests only).

## Collaboration views (web-m3, stage 3)
- Added `/collaborations` (list + propose form, simulated agents only, share entered as percent -> bps, local checks + server 422 banner), `/collaborations/[id]` (state, parties, contribution, share bps, expected payout, payout/net, ledger, activity; buttons rendered only from `actions`; fund 409 `approval_required` shows an explicit owner-approval confirm that re-sends `owner_approved: true`; cancel with optional reason), agent detail Collaborations section (replaces "not available"), Activity rows link to collaboration, nav item. Helpers + tests: `lib/collaborations.ts`.
- Chrome-verified on real backend: propose (with empty-form local errors) -> accept -> fund (approval prompt) -> approve -> complete (+1.80 / -1.80 net), and cancel with reason (created via curl, cancelled in UI).
- Not verified in browser: reject, cancel of a funded collaboration (refund), authorization_denied 422 display.

## Custom code editor (web-m3, stage 5)
- `custom.code` block in the builder renders a textarea editor (`components/builder/CodeEditor.tsx`): monospace, UTF-8 byte counter vs 64 KiB, starter template (also prefilled when the block is added), "Test run (dry run)" (`POST /agents/{id}/code/test-run` with the unsaved source; needs a saved agent) showing each intent with its authorize verdict, logs, fuel, ctx, labelled as simulated with no writes. Save uses the normal versioned edit; the 422 `graph.nodes.<id>` message shows on the node and in the banner. Agent detail shows a "runs custom code" badge and a note; `custom_code_failed` activity rows (agent detail and Activity) get a plain-language extra with the error and logs. Helpers + tests: `lib/customCode.ts`.
- Chrome-verified (real backend with sandbox): write strategy, test run (20 USDC denied: above max per interaction, log and fuel shown), save v2, tick -> proposal_denied; infinite loop save refused with inline 422; sample-passing/live-looping strategy saved as v3, ticks record one custom_code_failed and later ticks run on.

## Browser wallet + agent vaults (wallet-ui, chain-vaults step 6)
- Branch `wallet-ui` (from `chain-vaults` 2f03581). Handoff: `reports/handoffs/2026-11-17-wallet-ui.md`.
- Added `viem` (only new dep). Pure helpers + tests: `lib/chain.ts` (agentRef, calldata for createVault / USDC transfer / withdraw / setPaused / setLimits cross-checked with `cast calldata`, USDC amount and % math in bigint with 0.01 USDC gas reserve, add-chain params, Arc 20 gwei fee floor, wallet/revert error mapping), `lib/wallet.ts` (EIP-1193 switch -> 4902 add -> switch, tested with a fake provider), `lib/txlog.ts`, `lib/vault.ts`.
- UI: `WalletProvider` in layout; agent detail "Arc testnet vault" section (create + link, link by hash, live vault view with drift, allocate amount/%, withdraw, pause, caps, sync by hash, tx log with explorer links); Wallet page "Arc testnet" section (wallet USDC, per-agent vault table, totals). Ledger tx hashes link to the explorer for arc_testnet entries.
- Built against the endpoint contract in the brief (`/chain`, `/agents/{id}/vault`, `/vault/sync`); real backend endpoints not available in this worktree. Mock in `scripts/dev-mock-server.mjs`.
- Tests: 82 web tests pass; lint 0 errors (2 old warnings); build ok.
- Chrome-verified on the mock with a fake injected provider (no real tx): connect, wrong network -> switch adds Arc (params checked), real Arc RPC USDC balance read, vault view + drift, form validation, pause and create-vault simulated against Arc then rejected in wallet -> "Rejected in wallet", link by hash (bad hash, 409 open interactions, success switches agent to arc_testnet), sync note, 503 chain_unavailable, no wallet, chain disabled, `/chain` 404, backend unreachable.
- Not verified: a real MetaMask signature, receipt confirmation, backend recording against the real endpoints, revert decoding against a live vault (unit-tested only).

## Approach block + firms (approach-firms-web)
- Branch `approach-firms-web` (from d46a107). Spec: `docs/design/approach-and-firms.md` sections 1, 3, 4. Handoff: `reports/handoffs/2026-11-17-approach-firms-web.md`.
- Approach: pinned `approach.profile` node in the builder (no ports, not deletable, added with defaults when missing) + editor panel below the canvas: stance half-wheel (drag/click/arrow keys, firm-role outer ring), numeric sliders, derived numbers per section 1; market sunburst (categories/subcategories from `/markets/categories`, OR semantics), breadcrumb chips, structure/keyword/symbol chips, four range rows, debounced `/markets/preview`. Agent detail shows an Approach card and firm membership.
- Firms: `/firms` (list + "Simple agreements" link to `/collaborations`), `/firms/new` (lead limited to `firm_role = lead`, split slider 50-100%, task fee, roles with slots/weights, inherit or customise markets), `/firms/[id]` (stats, split bar, roles/slots, members, contributions, firm challenges via `firm_id`, join/leave/close driven by `actions`). Nav: Collaborations replaced by Firms. Activity: labels/tones/firm links for the new kinds. `TickReport.activated` typed (DevTick shows it).
- Pure helpers + tests: `lib/approach.ts`, `lib/firms.ts`, `lib/wheel.ts` (112 web tests). Mock: `scripts/dev-mock-server.mjs` (`MOCK_MARKETS=down`, `MOCK_FIRMS=0`, keyword `mockerror`).
- Chrome-verified on the mock only (real backend not in this worktree). Not verified: real backend, drag gesture (click + keys verified), mobile layout.
