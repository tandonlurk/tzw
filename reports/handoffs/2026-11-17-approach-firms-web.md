# Handoff: approach block + firms UI (2026-11-17)

Branch `approach-firms-web`, from `d46a107` (design doc). Spec: `docs/design/approach-and-firms.md` sections 1, 3 and 4.

## What was done
- **Approach editor** (`web/components/approach/*`), shown in the builder as a panel under the canvas. It opens when the pinned Approach node is selected, and is open by default.
  - Stance half-wheel. Angle runs from competitive (left) to collaborative (right) and sets `collaboration_bps`. Radius runs from cautious (centre) to ambitious (edge) and sets `ambition_bps`. Values snap to 100 bps. You can drag, click, or use the arrow keys (500 bps per press, 100 with Shift; Home/End jump to the ends).
  - An outer ring sets `firm_role`: "No firm" covers the left half, and "Join firms" and "Lead a firm" split the right half. Sliders, number inputs and radios give the same controls without the wheel.
  - Derived numbers follow the section 1 formulas: stake target from the form's live `max_per_interaction`, default min conviction, max open deals, starts challenges / joins firms / can lead. A warning appears when a role is set but collaboration is below 30%.
  - Market sunburst. The centre is "Any market" and clears all filters. The inner ring is categories and the outer ring is subcategories, both from `GET /markets/categories`. Clicking a wedge toggles it, and a checkbox "List view" offers the same choices.
  - Below the wheel: breadcrumb chips, structure chips, keyword and symbol chips, and four min/max range rows. Each row has a number input in display units (% or hours) plus a slider, and checks min ≤ max locally.
  - Live preview calls `POST /markets/preview`, debounced 400 ms. It shows the count, up to 10 samples and the data source, with error/retry and empty states.
- **Builder**:
  - The `approach.profile` node is pinned: `deletable: false`, no ports, the palette doesn't list it, and `remove` ignores it.
  - A graph without an approach node gets one with default params (id `approach`, or `approach_N` if that id is taken). A notice says it will be added on save.
  - Params are written in the exact section 1 shape (`approachToParams` → `serializeMarkets`).
  - 422 `graph.nodes.<id>…` details appear on the node and in the panel.
- **Agent detail**: an Approach card with a summary and the derived numbers. It also says "Leads / Member of" a firm, based on `GET /firms`.
- **Firms**:
  - `/firms`: cards with lead, members/slots, split bar, markets, task fee, lead P&L and paid to members, plus a "Simple agreements" link to `/collaborations`.
  - `/firms/new`: the lead dropdown lists only simulated agents whose approach `firm_role = lead`; if there are none, an empty state links to the agents' edit pages. Also on the form: name, mandate (500 characters max), split slider (50–100% lead, 1% steps), task fee, roles (slots 1–5, weights must total 100%, checked live), and markets (inherit = `null`, or customise with the market editor). Server 422 details appear inline per field.
  - `/firms/[id]`: stats tiles, split bar, roles and slots, markets, members, contributions table (used links to the interaction, fee, payout), firm challenges (the lead's interactions filtered by `firm_id`). Join/leave/close buttons only appear when `actions` contains them.
- **Nav**: Collaborations is replaced by Firms, which is also highlighted on `/collaborations`. The Collaborations page is retitled "Simple agreements".
- **Activity**: readable labels and tones for `firm_*` and `approach_filtered`, a "firm" link (taken from `firm_id` or `detail.firm_id`), and plain-language extras for task fee, payout, fee denied and filtered.
- **Tick report**: `TickReport.activated?` added. DevTick already prints every non-zero counter, so it shows `activated: N`.
- **Mock server** (`web/scripts/dev-mock-server.mjs`, labelled mock throughout):
  - Approach block in the catalog, `/markets/categories` (source `fallback`), and `/markets/preview` (source `fake`, using the agreed OR/AND semantics).
  - The firms endpoints with 422/409 errors, a seeded firm with contributions and a settled firm challenge, and the new activity kinds.
  - Approach range errors on validate and save.
  - Switches: `MOCK_MARKETS=down`, `MOCK_FIRMS=0`, and the preview keyword `mockerror` (returns 503).

## How to run / test
- `cd web && npm ci && npm test && npm run lint && npm run build`
  - 112 tests pass.
  - Lint: 0 errors, 1 pre-existing warning in `lib/graph.test.ts`.
  - Build passes.
- Mock: `PORT=8093 npm run dev-mock`, then `NEXT_PUBLIC_API_URL=http://localhost:8093/api npx next dev -p 3093`.

## Browser-verified (Chrome, mock only)
- **Stance and derived numbers**:
  - Clicking the wheel and the arrow keys both move the marker, and the numbers update. At a=9300 with max 5 USDC: 4.72 USDC, 51.75%, 4.
  - Clicking the firm-role ring works.
  - The role-inactive warning shows.
- **Pinned node**: it survives Backspace/Delete. The "no approach block" notice shows on an agent without one.
- **Market editor**:
  - Wheel: toggling a category or subcategory works.
  - Breadcrumb: the subcategory chip shows its parent category.
  - Preview narrows correctly: Crypto OR Energy, keyword oil, move ≥ 3% → 1 market.
- **Range validation**: setting min > max shows a local error and blocks the preview. Saving anyway shows the server 422 in the banner, on the node and in the panel. Fixing it and saving gives v2, and the agent page Approach card updates.
- **Firms**:
  - List page, plus the empty state with `MOCK_FIRMS=0`.
  - Detail page tables and the firm challenge.
  - Join as analyst, join a scout until the firm is full (the Join form disappears), leave, close (buttons gone, closed notice).
  - Create with weights at 120% shows the error. Fixed weights, custom markets (BTC, move ≥ 4% → 2 matches) and an 85% split creates the firm and redirects. Creating a second open firm for the same lead shows the 422 inline.
- **Error states** (`MOCK_MARKETS=down`): categories error with retry, preview error with retry. Keyword `mockerror` also shows the preview error.
- **Activity page**: the new kinds render with labels and firm links.

## Not verified
- Anything against the real backend: the backend agent's branch had no commits when I checked.
- A pointer drag on the stance wheel. It uses the same code path as click; only click and keys were exercised.
- Narrow/mobile layout.
- Unreachable backend on the new pages. They use the shared `ErrorState`.
- A 409 (`no_free_slot` / `already_member`) shown in the browser. It goes through the same `ErrorBanner` path as the 422, which was verified.

## Assumptions about the API (please confirm in the backend)
1. **Approach node**: id is free-form (I use `approach`). Params are sent complete: every list is present, and every range bound is an explicit `null` or an integer. `scope` is derived by the UI: named markets → `specific`; only structure or ranges → `structure`; otherwise `any`. Keywords are sent lower-case and symbols upper-case.
2. **Catalog**: `approach.profile` has `kind: "approach"` with empty `inputs`/`outputs`. If the catalog lacks it, the UI renders the node anyway and warns that the save may be rejected.
3. **422 paths for approach params**: `graph.nodes.<id>.params...` (as for other blocks). For firms, `details[].path` (or `field`) of `lead_agent_id`, `name`, `mandate`, `lead_share_bps`, `task_fee`, `roles` or `markets` is shown under that field. Anything else goes in the banner.
4. **`MarketView` in the preview sample**: only `title` is required. When present, `category`, `subcategory`, `structure`, `move_bps`, `yes_price_bps` and `expiry_secs` are displayed.
5. **Firm `actions` values**: `"join"`, `"leave"`, `"close"`. Join/leave send `{agent_id, role}` and `{agent_id}` as in section 3.
6. **Firm activity items**: carry `detail.firm_id` (a top-level `firm_id` also works) and, for fees/payouts, `detail.amount` (or `fee`/`payout`) as a money string.
7. **`Interaction.firm_id`**: present on the lead's interactions. Firm challenges are read through `GET /interactions?agent_id=<lead>`, filtered by `firm_id`. There is no `firm_id` query filter.

## Proposed decisions (W-8..)
- **W-8**: The approach editor lives in a panel under the builder canvas, open by default. It is not a narrow side column, because the two wheels need the width. The canvas node is a compact summary with an "Edit approach" button.
- **W-9**: Stance wheel layout. It is the upper half-disc. The outer ring's left half is "No firm"; the right half is split into "Join firms" (c 50–75% of the arc) and "Lead a firm" (75–100%). Ring position is visual only: picking a role never moves the stance point. Snap is 100 bps; keys move 500 bps, or 100 with Shift.
- **W-10**: Market scope is derived, never chosen. The "Any market" centre clears every filter, including ranges, keywords and structures, so a hidden filter can never sit under "any".
- **W-11**: Range display units. Move, YES price and volume change are entered in % (2 decimals, converted to integer bps exactly). Expiry is entered in hours (2 decimals, rounded to whole seconds). Slider spans: move 0–20%, YES 0–100%, expiry 0–30 days, volume −100% to +500%. Typed values may go beyond the slider.
- **W-12**: The firm split bar shows "lead | scout | analyst" as if every role contributed, with the note that only contributing roles share the pool. The display math (`splitProfit`) follows the decided rule: renormalise over contributing roles, equal within a role, floor, remainder to the lead.
- **W-13**: The firm create form defaults to roles scout ×2 at 50% and analyst ×1 at 50%, task fee 0, and an 80% lead share. The roles are always sent explicitly.
- **W-14**: Owners can add any simulated agent to a firm from its page. An amber note appears when that agent's approach isn't "Join firms", and the backend decides whether that is allowed.

## Unfinished / follow-ups
- Integration run against `approach-firms-backend` once it lands: check the assumptions above, especially the 422 path shapes and the catalog `kind`.
- No dashboard tile for firm income yet. Members' `firm_income` is only visible on the firm page.
- The agent edit form does not warn when an agent that leads an open firm is switched away from "Lead a firm".
- The mock's `/activity` ignores the `agent_id` filter, so agent pages on the mock show all activity. This is a mock-only issue.

## Note
- During an early smoke test I ran `pkill -f dev-mock-server.mjs`. It may have stopped another worktree's mock server, if one was running at the time.
