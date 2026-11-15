# API contract (v0, M1 scope)

Backend owns this file. Frontend builds against it. Change it in the same commit as the code.

Conventions: base `http://localhost:8080/api`; JSON; ids are UUID strings; timestamps RFC 3339; **money is a string of integer USDC minor units** (6 decimals, `"1500000"` = 1.5 USDC); `env` is `"arc_testnet"` or `"simulated"`; ratios are integer basis points (0–10000). Errors: HTTP 4xx/5xx with `{"error":{"code":"...","message":"...","details":[...]}}`. Single demo user, no auth (D-9).

## Types

```
AgentConfig  { goals, strategy, categories: string[], horizon_secs, risk_tolerance: "low"|"medium"|"high", schedule_secs }
AgentPolicy  { max_per_interaction: money, max_aggregate_exposure: money, approval_threshold: money,
               allowed_interaction_types: ("challenge"|"collaboration")[], min_conviction_bps, stop_loss: money|null }
Graph        { schema_version: 1, nodes: [{id, type, params: object}], edges: [{from, from_port, to, to_port}] }
Agent        { id, name, description, status: "active"|"paused", env, wallet_address: string|null, version: int,
               config: AgentConfig, policy: AgentPolicy, graph: Graph,
               balances: { available: money, committed: money },
               stats: { realized_pnl: money (signed), settled: int, wins: int }, created_at, updated_at }
AgentInput   { name, description, config, policy, graph }        // create and update body
Template     { id, name, description, config, policy, graph }
BlockDef     { type, kind: "trigger"|"reasoning"|"condition"|"action", label, description,
               inputs: [{port, data_type}], outputs: [{port, data_type}], params_schema: JSON Schema }
GraphError   { code, message, node_id: string|null }
LedgerEntry  { id, ts, env, account, delta: money (signed), reason, interaction_id: string|null, tx_hash: string|null }
ActivityItem { id, ts, agent_id: string|null, kind, summary, detail: object|null }
```

## Endpoints

| Method | Path | Body → Response | Notes |
|---|---|---|---|
| GET | `/health` | → `{status:"ok"}` | |
| GET | `/agents` | → `Agent[]` | |
| POST | `/agents` | `AgentInput` → `Agent` (201) | Validates policy and graph; 422 with details on failure |
| GET | `/agents/{id}` | → `Agent` | |
| PUT | `/agents/{id}` | `AgentInput` → `Agent` | Creates a new immutable version; history untouched |
| POST | `/agents/{id}/status` | `{status}` → `Agent` | pause / resume |
| GET | `/agents/{id}/versions` | → `[{version, created_at, config, policy, graph}]` | |
| GET | `/agents/{id}/ledger` | → `LedgerEntry[]` | newest first |
| POST | `/agents/{id}/allocate` | `{amount: money}` → `Agent` | Moves user unallocated → agent available. `simulated` env only in M1 |
| POST | `/agents/{id}/deallocate` | `{amount: money}` → `Agent` | Reverse; fails if amount > available |
| GET | `/templates` | → `Template[]` | |
| GET | `/graph/blocks` | → `BlockDef[]` | The block catalog the builder renders |
| POST | `/graph/validate` | `Graph` → `{valid: bool, errors: GraphError[]}` | Always 200 |
| GET | `/wallet` | → `{env, unallocated: money, allocated: money, committed: money, entries: LedgerEntry[]}` | User-level overview |
| POST | `/wallet/simulated-deposit` | `{amount: money}` → wallet | Credits simulated funds, labelled as such |
| GET | `/activity?agent_id=&limit=` | → `ActivityItem[]` | newest first |

M2+ (not yet specified): `/interactions`, `/approvals`, `/stats/portfolio`, `/webhooks/quicknode`, `/dev/tick`.
