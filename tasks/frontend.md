# Frontend / UI

## Public Forum (2026-11-20)

- Scope/assigned: frontend specialist, integrated by lead as `371f1c8`. Files: shared `/forum` page, `lib/forum.ts`, API methods, shell navigation/breadcrumbs/mode regression assertions; clarified Firms/Challenges intro text.
- Done: Network → Public Forum in both modes, categories, real persisted thread/reply forms, selected agent attribution, explicit fictional demo badges and local-prototype notice. No static fake engagement or autonomous posting claims.
- Validation:168tests,TypeScript,targetedlint,productionwebpackbuild pass. Lead verified /forum,/wallet,/agents,/interactions,/firms HTTP200 and actual forum/portfolio API through frontend proxy. No new browser interaction test claimed.
- Dependency/limits: backend1d020f1 running8090; six seeded demo threads. Owner posting on behalf of an agent is not autonomous authorship. Public hosting/identity/moderation out of scope. Seeded portfolio provides three root cards and grouped clones. Another session's dashboard/navigation WIP is preserved.

## Grok Bot adversarial practice (2026-11-20)

- Objective/scope: persistent mock-adversary practice UI against the agreed backend contract; assigned frontend specialist, integrated by lead on `integration-1119` as `84a5058`.
- Status: implementation complete; live Bot submission pending owner setup. Depends on D-75 backend endpoints and restricted credential.
- Files: Basic/Advanced agent Sparring routes, `components/agent/SparringWorkspace.tsx`, workspace tabs, `lib/sparring.ts` and tests, API client, mode mapping.
- Completed: create/list sessions, frozen strategy/version display, critique/source links, owner replies/revisions, safe prompt copy and 10-second visible-page refresh. Drafts survive reloads/errors; per-agent state is isolated. No live configuration or financial edits.
- Validation: 168 web tests passed, TypeScript passed, targeted lint clean, production webpack build passed. Read-only integration/security review found and verified fix for polling that erased drafts; no unresolved high/medium findings.
- Assumptions/limits: source records identify the submission connection, not verified model identity; evidence claims remain unverified. No canned Bot replies, RL training, backtests, automatic rebuttal or activation.
- Next: configure Grok Bot access/tunnel/restricted token and record a real critique. Setup: `docs/setup/grok-bot-sparring.md`. Another agent owns `demo-video/`; none of its files were edited.

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

## TZW UI redesign (ui-1119, overnight 2026-11-19)
- Branch `ui-1119` (from `afternoon-1118` 7572fa2). Brief: owner's "TZW — UI/UX Design & Implementation Brief". Notes: `docs/design/ui.md`, tips `docs/design/ui-tips.md`, decision D-72. Handoff: `reports/handoffs/2026-11-19-ui-overnight.md`.
- Shell + tokens (OKLCH charcoal, blue/yellow accents, Inter + JetBrains Mono), sidebar, breadcrumbs, mode switch with context-preserving routes (`lib/mode.ts`). Basic: Overview, My Agents cards, agent workspace tabs, creation wizard, sectioned configuration, wallet, activity timeline, settings. Advanced: Terminal (ticker, presets, saved layouts, drag/resize panels), agents table, agent overview/performance/logic/code/activity/settings, history table + drawer. Logic canvas replaces the in-form builder; code workspace with versions and dry-run output.
- Tests: web 154 pass (new: mode, series, layouts, activityKinds, blockVisuals, jsHighlight, money leniency); lint 0 errors (1 old warning); `next build` OK.
- Chrome-verified on the real backend (fresh `aen_ui` db, demo + firm seeds, 4 rounds): every page above at 1440 wide; 900 and 520 wide for Overview, Terminal, My Agents; canvas select/panel, block menu search + Enter add, live validation, discard; terminal hover tooltip, presets; code workspace add block + dry run; approve from Approvals; Dev tick from Terminal; mode switch by icon and ⌘\ keeps the agent and tab.
- Also verified: canvas edit + ⌘S saved v2 (min conviction 6000 → 6500), Configuration save v3 kept that graph (API checked), terminal corner-resize by mouse (2×1 → 4×1, persisted across reload).
- Not verified: panel drag-reorder by mouse (HTML5 drag; logic unit-tested), wallet vault UI with MetaMask, Arc testnet terminal view (no vault in this db).

## Landing page + Devpost copy (2026-11-20)
- Objective/scope: owner-requested default landing page referencing the supplied layout, monochrome ASCII rendition of `demo-video/assets/logos/final.jpg`, stationary hands and rotating coin. Separate submission copy beside the voiceover script.
- Assigned: lead. Dependencies: existing Next.js shell/tokens, owner's logo. No new dependencies.
- Files: `web/app/page.tsx`, `landing.css`, preserved Overview at `app/dashboard/page.tsx`; `components/landing/` texture + canvas renderer; reproducible `scripts/build-landing-ascii.mjs`; shell/breadcrumb/nav/mode routing; `demo-video/devpost-responses.md`.
- Completed: black/white character art, coin rotation, responsive banner with TZW wordmark and blue Get started buttons; dashboard remains available at `/dashboard`, Advanced switch returns there. Landing has no sidebar or approvals polling. Copy contains elevator pitch, About sections, project description and technical summary; script untouched.
- Validation: 168 frontend tests, TypeScript and targeted ESLint pass. Production webpack build passed. Headless Chrome desktop 1440 and mobile 390: no overflow, no runtime exceptions; pixel comparison found zero changed hand pixels and changing coin pixels; reduced-motion freezes the art. Direct dashboard shows the existing shell and real backend data.
- Decision/assumptions: local repository work only, no publication. Source `final.jpg` is the requested `final.jpeg`. Silhouette masks exclude the fresco background. Full-page Get started links enter the owner console.
- Limitations: artwork uses approximate hand silhouette masks and a projected coin texture, not a 3D mesh. No financial/business logic changed. No blockers.
