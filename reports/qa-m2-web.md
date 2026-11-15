# QA M2 web

Verified in Chrome against the real backend (fake market): approvals queue (approve both sides -> active), settle via /dev/tick, dashboard totals (allocated 200.00, per-agent P&L +15/-15, outcomes 5 settled/2 cancelled) equal `/stats/portfolio`, interaction detail, builder add/connect/incompatible.

Not browser-tested: reject, approve-then-cancelled message, agent detail challenge list, create-form description prefill.

## Backend/API observations (no edits made)
1. Negotiation summaries contain raw units ("stake 10000000 minor units", "at or above 5000"), shown as-is on the detail page.
   `curl -s $API/interactions/$ID | jq '.negotiation[].summary'`
2. Agents with a lowered `approval_threshold` only ask for approval when conviction is high (stake sizing stays at the threshold otherwise), so a demo needs `POST /dev/fake-market {"move_bps":700}` before the propose tick.
3. Simulated clock jumps (`/dev/tick {"now":...}`) of many minutes cancel in-flight negotiations ("funding deadline passed"); tick in small steps.
4. With one open challenge on the fake market, no further proposals are made (not an error, but looks like agents are idle).
