# TZW UI — design notes

Implements the owner's "TZW — UI/UX Design & Implementation Brief" (2026-11-19). Decisions: D-72. UI tips: `ui-tips.md`.
Everything here is presentation; the API, ledger and permission rules are unchanged.

## Identity

- **Palette** (`web/app/globals.css`, OKLCH): charcoal `#171717 / #1D1D1D / #242424 / #303030 / #393939`, text `#F1F1F1 / #C4C4C4 / #999`,
  soft blue `#A9CFF5` = interactive and selected, muted yellow `#E7BD58` = money at stake, pending, attention.
  Mint / rose are kept only for gains / losses. Borders are translucent white (8% / 13%). Elevation = lighter surface.
- **Type**: Inter (UI, tabular numbers) + JetBrains Mono (axis labels, ticker, code, ids), via `next/font`.
- **Wordmark**: `TZW` in mono, letter-spaced, with a small yellow tick. Top-left of the sidebar.
- **Agent colours**: 8 hues at one lightness/chroma (`--agent-0..7`), assigned by sorted agent id (`lib/agentColors.ts`), so
  an agent has the same colour on every chart and avatar. Past 8 agents, lines repeat colours with dashes.
- **Icons**: hand-drawn 24px / 1.5px stroke set in `components/shell/icons.tsx` (no dependency).

## Shell

- Persistent left sidebar (no top nav). Basic: labelled groups (Workspace, Network, Assist) + Settings at the bottom; collapses
  to an icon rail under 1080px. Advanced: always an icon rail with small labels.
- Slim utility bar: breadcrumbs left; pending-approvals pill and the **mode switch** (single icon, blurred icon swap) right.
- Mode switch keeps context (`lib/mode.ts`): `/agents/:id/logic` ↔ `/advanced/agents/:id/logic`, `/` ↔ `/advanced`, etc.
  Shared pages keep their URL and render in the last mode (cookie `tzw-mode`). Shortcut ⌘\ / Ctrl+\.
- Sidebar footer always says the funds are simulated or testnet.

## Basic mode

| Page | What it shows |
|---|---|
| Overview `/` | Portfolio value (unallocated + allocated), realized P&L, funds-by-state bar, P&L-by-agent chart, "Needs your attention" (approvals + co-captain), agents, recent activity |
| My Agents `/agents` | Agent cards: monogram, purpose, status, available / committed / P&L, P&L sparkline, last event; "Create an agent" card |
| Agent `/agents/:id` | Tabs Overview · Logic · Activity · Configuration. Overview: KPIs, open challenges, recent decisions, risk vs limits, funding, vault, approach |
| Create `/agents/new` | Wizard: Template → Identity → Goals and strategy → Risk → Permissions → Automation → Review |
| Configuration `/agents/:id/edit` | Same sections on one page with a section index and a sticky save bar ("Save as version N") |
| Wallet, Activity, Settings | Funds by state + per-agent table + filtered ledger; day-grouped timeline with category / agent filters; browser preferences |

## Advanced mode

| Page | What it shows |
|---|---|
| Terminal `/advanced` | Ticker (value, realized, at risk, free, open/active, unrealized n/a), environment switch, live refresh, layout presets (Focus, Overview, Compare 2×2, Risk) and saved layouts; panels can be added, removed, re-pointed to another metric, dragged to reorder (grip) and resized (corner, snaps to the 4-column grid) |
| Agents `/advanced/agents` | Sortable dense table with exposure used and trend sparkline |
| Agent `/advanced/agents/:id` | Tabs Overview · Performance · Logic · Code · Activity · Settings, identity + KPIs always visible |
| Logic / Code `/advanced/logic`, `/advanced/code` | Open the last agent's canvas / code, or pick an agent |
| History `/advanced/activity` | Filterable event table with a detail drawer (raw record) |

Panel metrics (`lib/layouts.ts`): P&L over time (agents + dashed portfolio line), single agent P&L, P&L by agent, capital
allocation, outcomes, win/loss record, agents table, recent activity. Every number comes from `/stats/portfolio`,
`/agents` or `/activity`; nothing is generated in the browser.

## Charts

`components/charts/TimeChart.tsx`: step lines (a cumulative ledger value holds until the next event), event dots, nice
y ticks on the right, local-time x ticks, hover crosshair + tooltip with every visible series, drag to zoom (double-click
or "Reset zoom"), ranges 1H/6H/1D/1W/All (ending at the later of now and the last event, because the simulated clock can
run ahead), legend toggles (Alt-click = solo). Same-timestamp points collapse to the last value.

## Logic canvas

`components/canvas/*`. One canvas per agent. Dotted near-black background, curved arrowed edges. Blocks are compact
shapes with the label underneath; family → shape: trigger circle (blue, "Start" tag), reasoning rounded square (lavender),
condition diamond (yellow; "yes" right, "no" bottom), action hexagon (mint), financial octagon (rose), market & approach
parallelogram (steel), custom code square, end capsule. Selecting a block opens the right panel (description, ports,
params form / approach wheels / code editor, Revert, Duplicate, Delete, Done). Add blocks with **/**, the Add block
button, drag from the menu, or by dropping a connection on empty canvas (menu filtered to compatible blocks; connects
automatically). Live server validation, problem list with jump-to-block, blocks not reachable from a trigger are dashed
and flagged, minimap, legend, ⌘S save as a new version, ⌘D duplicate, Esc closes.

## Code workspace

`components/code/CodeWorkspace.tsx`. Files = the agent's `custom.code` blocks; versions list (read-only view, restore into
draft); editor with line numbers and a small JS highlighter (escapes first); status bar (bytes vs 64 KiB, cursor,
problems); Output panel = backend dry run (`/code/test-run`, writes nothing) and Problems = local checks + server
validation. ⌘↵ test run, ⌘S save.

## Not done / next

- No undo/redo on the canvas; no multi-select actions beyond React Flow's box select + Delete.
- Terminal resize is grid-snapped by corner drag or the "wider" button; there is no free-form placement.
- Arc testnet terminal view only appears when an agent has a vault (button disabled otherwise).
- ElevenLabs / voice co-captain has no UI yet (brief priority 5).
- Light theme: none (brief asks for dark only).
