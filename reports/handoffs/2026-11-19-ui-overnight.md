# Handoff 2026-11-19 — overnight UI groundwork (TZW brief)

Branch **`ui-1119`** (from `afternoon-1118` 7572fa2). Local commits only: **not pushed, not merged**. Review, then merge.
Decision D-72 (default, review). Design notes `docs/design/ui.md`; your UI tips saved as `docs/design/ui-tips.md`.

## State

- Web: `npm test` 154 passed (was 147 at start of night; +7 files of pure-logic tests), `npm run lint` 0 errors, `npm run build` OK.
- Backend untouched. No API, ledger or permission change. Scratch db `aen_ui` (demo + firm seeds, 4 rounds, one agent "Night Owl"
  made with the wizard). Servers may still be up (backend :8080, `next dev` :3000); if not:
  `cd backend && DATABASE_URL="postgres://$USER@localhost/aen_ui" MARKET_DATA=fake TICK_INTERVAL_SECS=0 cargo run -p aen-server`
  and `cd web && npm run dev`. `caffeinate` runs until ~14:00.

## What you can look at (open http://localhost:3000)

1. **Basic** (`/`): Overview, My Agents → an agent (tabs Overview · Logic · Activity · Configuration), New agent wizard, Wallet, Activity, Settings.
2. **Mode switch**: icon top right, or ⌘\. On an agent page it opens the same agent and tab in the other mode.
3. **Advanced** (`/advanced`): Terminal (try presets, Save layout, Add panel, drag a panel by its grip, drag its corner,
   hover and drag-zoom the chart), Agents table, an agent's Performance and **Code** tabs, History (click a row).
4. **Logic canvas** (agent → Logic): press `/`, search, Enter; click a block for the side panel; drag from a block's
   output onto empty canvas to get a filtered menu that auto-connects.

## Judgement calls (the brief vs what exists)

- Kept Challenges, Firms, Approvals and Co-captain in the sidebar (the brief listed 5 Basic items; these pages carry the demo).
  Grouped as Workspace / Network / Assist so the list stays short.
- Logic moved out of the Configuration form into its own tab (one canvas per agent, as the brief asks). Configuration
  saves leave the graph untouched; the canvas saves its own version.
- Basic and Advanced share URLs for wallet/challenges/firms/approvals/settings (they follow the last mode) instead of duplicating pages.
- "Avatar or visual identifier": an automatic monogram in a stable per-agent colour; no upload (nothing to store it in).
- Unrealized P&L is shown as "n/a" everywhere (no fair mark; same as before). Charts show only ledger data.
- Block shapes map to real backend kinds: `action.request_approval` is the "financial" octagon, `approach.profile` the
  parallelogram, `custom.code` a square, `action.reject` the "end" capsule.

## Not done / not verified

- Verified in Chrome: canvas edit + ⌘S → v2; Configuration save → v3 with the canvas change kept (checked in the API);
  approve from Approvals; Dev tick from the Terminal; terminal corner-resize; layout persists across reload.
- Not verified: panel drag-reorder with a real mouse (HTML5 drag; logic unit-tested), Arc testnet terminal view, MetaMask vault flows.
- No canvas undo/redo; ElevenLabs voice UI not started (brief priority 5).
- `docs/demo.md` got a rename note at the top; the step tables still use the old page names.

## Needs your decision

1. Review D-72 and merge `ui-1119` (it sits on top of `afternoon-1118`, which also is not on `main` yet).
2. Still open from before (TODO "Start here"): confirm MetaMask on Arc testnet and try the wallet UI.
