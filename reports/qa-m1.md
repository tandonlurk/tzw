# QA M1: web frontend vs real backend

Setup: backend built from this worktree (`aen-server`, PORT=8090, CORS_ORIGIN=http://localhost:3010, db `aen_qa`), `next dev -p 3010` with `NEXT_PUBLIC_API_URL=http://localhost:8090/api`, driven in a real Chrome via browser tools.

Static: `npm ci`, `npm test` (15 pass), `npm run lint` (0 errors, 2 unused-var warnings), `npm run build`, `tsc --noEmit`: PASS.

| Item | Result |
|---|---|
| Wallet shows simulated label, simulated deposit 100 -> "100.00 USDC", ledger row | PASS |
| Create from template: Momentum bull, Skeptic, Cautious (all 201, redirect to detail) | PASS |
| Local validation (empty name) blocks submit with highlighted fields | PASS |
| Builder: template graph renders, positions come from backend `ui` | PASS |
| Builder: delete block, Validate shows node-level errors (unreachable, dead_end) outlined on nodes | PASS |
| Builder: drag node, save, reload: position persisted by backend (`ui` x=48,y=244), version 2 created, versions list [2,1] | PASS (new) |
| Builder: add block / connect / incompatible connection rejection | NOT browser-tested; `canConnect` unit-tested only |
| Server 422 -> field mapping (`policy.approval_threshold` shown under the field) | PASS |
| Server 422 graph paths `graph.nodes.<id>` mapped onto nodes, `graph` errors in the graph panel | FIXED (was last-one-wins in one field, nodes not marked) then PASS |
| Allocate 500 (over): "insufficient_funds" shown, balance unchanged | PASS |
| Allocate 25.5 (curl) / Deallocate 10.5 (UI): 15.00 shown, ledger rows correct | PASS |
| Pause / Resume toggles status | PASS |
| Edit creates new version (v2), history kept | PASS |
| Dashboard: unallocated 85 + allocated 15 = 100 deposit; P&L/settled 0 (no invented values) | PASS |
| Activity feed readable, newest first, matches actions | PASS |
| Money formatting ("1500000" = 1.50 USDC, 10.5 -> 10500000) | PASS |
| Backend stopped: "Backend unreachable" with Retry | PASS |

## Fixes made
- Node positions now travel in graph `nodes[].ui` (priority: node ui > localStorage > auto layout).
- 422 `details[{path,code,message}]`: multiple errors per field joined; `graph.nodes.<id>` shown on the node.

## Observations / backend notes
- No backend bugs found. 422 `details[].path` is dotted (`policy.max_per_interaction`, `graph.nodes.<id>`, `graph`); worth documenting in `docs/api.md`.
- Repro of the 422 shape: `curl -s -XPOST localhost:8090/api/agents -H 'content-type: application/json' -d '{...policy.approval_threshold > max_per_interaction...}'`.
- Create form: template description is not prefilled into the Description field (left empty; name is required). Minor.
- Ledger for allocate only lists the agent-side account entry in the agent ledger (expected).
