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

## Collaboration pass (stage 3)
Via UI on a fresh db: propose (empty submit shows local errors) -> accept -> Fund showed the owner-approval notice -> "I approve: fund now" -> funded -> "Declare complete and pay out" -> net +1.80 / -1.80, ledger and activity correct. Cancel with reason on an unfunded one (created by curl) -> cancelled, no actions left. Agent detail and Activity link to collaborations. Not tested: reject, funded cancel, authorization_denied display.

## Stage 4 pass (approvals, collab reject/cancel/denial, 127.0.0.1, mobile)
Verified in Chrome: approve one challenge approval and reject another from /approvals (notices correct; reject says no funds moved); reject a collaboration (final, no actions); fund with owner approval then cancel funded (ledger shows collab_commit then collab_refund; agent balances back to 102.50 / 97.50 USDC available, 0 committed); fund denied by authorize (Skeptic paused) shows `authorization_denied` banner with "partner: ... the agent is paused", page stays usable; 390px (iframes) Dashboard, Agents, Collaboration detail: no horizontal overflow, layout stacks.

Bugs fixed (web)
- Opening `http://127.0.0.1:3013` in dev left every page on "Loading..." forever (Next dev blocks hydration from a non-localhost origin). Added `allowedDevOrigins: ["127.0.0.1"]` in `web/next.config.ts`.
- With the API refusing that Origin (403 origin_not_allowed, surfaced by the browser as a failed fetch) the message was "backend unreachable, check it is running". Now says it may have refused the page origin, shows the page origin and CORS_ORIGIN hint (`lib/errors.ts`, tested).

Backend notes (not changed)
- The browser cannot read the 403 body for a CORS/origin refusal, so the web app cannot distinguish it from a dead server; the hint covers both.
- A second challenge could not be proposed while another was active between the same pair (even with PROPOSAL_COOLDOWN_SECS=0); fine, but approvals queue could not be filled with two at once in the demo.

## Custom code pass (stage 5)
Via UI: strategy written, dry-run showed `propose_challenge` 20.00 USDC Denied (above max per interaction) + log + fuel; saved v2 ("Saved as version 2", "runs custom code" badge); tick gave `proposal_denied` in agent activity. `for(;;){}` save refused with inline 422 ("did not run on a sample context: strategy used too much CPU"). Strategy that loops only on non-sample ctx saved as v3; ticks (simulated clock) recorded one `custom_code_failed` (shown with plain-language text) and further ticks completed normally. Test run is disabled while creating (no agent id yet).
Note: `/dev/tick` without `now` did not run the schedule trigger twice within 60 s; used `now` offsets.
