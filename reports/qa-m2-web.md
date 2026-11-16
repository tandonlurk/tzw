# QA: web M2/M3 pass (web-m3)

Run: fresh db, API :8094 (fake market), web :3013, backend/README Demo script incl. co-captain rounds, one Chrome tab (closed).

Verified in browser
- Dashboard: pending co-captain count banner (2) links to /cocaptain; P&L chart by agent (-7.50/+7.50), allocation, outcomes (3 settled) match API.
- Co-captain: both rule suggestions listed; lower_max_stake shows Max per challenge 10.00 -> 5.00; approval-threshold one shows loosens-control warning. Accept created agent version 2 (Activity shows "suggestion accepted" and "configuration changed (version 2)").
- Agent detail (Skeptic): version 2, 0 of 3 won, unrealized "n/a", version history v2 diff "Max per interaction 10.00 -> 5.00".
- Edit: max_per_interaction 999 > aggregate gave server 422 shown inline (`policy.max_per_interaction ... must not exceed`); fixing to 4 saved, landed on `?saved=3`, "Saved as version 3" notice and v3 in history.
- Interaction detail: decision trace shows "(config v1)" for pre-edit runs; Approvals (empty state after approval), Challenges list, Activity all render.

Bugs fixed
- Dashboard "Settled / wins" showed 6 for 3 challenges (counted per side); relabelled with a note.

Observations (not fixed)
- Agent detail repeats identical-looking challenge cards (same market/outcome); correct, just visually repetitive.
- Edit page: mouse wheel over the builder canvas zooms the canvas instead of scrolling the page.
- Reject path and ledger/versions after simulated-clock jumps not re-tested this pass.
