# Approach block and firms — design (2026-11-17)

Owner request (2026-11-17): every agent gets a built-in **approach** that decides how it goes at the
market: how competitive vs collaborative, how ambitious, and which markets (from "any market" down to
"energy markets with a >3% move"). Collaboration becomes **firms**: a lead agent opens a firm, keeps
most of the profit (default 80%), and the remaining pool (default 20%) goes to member agents that do
tasks for it. Members earn without orchestrating everything; the lead profits most.

All numbers below are defaults (AGENTS.md 7.6): isolated, documented, easy to change. Firms are
simulated-env only for now (payouts are ledger transfers; on Arc they would need vault-level transfers).

## 1. Approach block

Graph node type `approach.profile`, new `BlockKind::Approach`, **no ports**, at most one per graph.
The validator exempts it from connectivity rules. The builder pins it (always present, cannot be
deleted; "reset to defaults" instead). A graph without one behaves as the defaults below, so every
existing agent keeps working. Starter templates gain one.

```jsonc
{
  "stance": {
    "collaboration_bps": 3000, // wheel angle: 0 = pure competitor, 10000 = pure collaborator
    "ambition_bps": 5000       // wheel radius: 0 = cautious, 10000 = ambitious
  },
  "firm_role": "none",         // "none" | "member" (open to joining firms) | "lead" (runs a firm)
  "markets": {
    "scope": "any",            // "any" | "structure" | "specific" (UI hint; filters below are what count)
    "structures": [],          // [] = all; "binary_up_down" | "price_ladder" | "categorical"
    "categories": [],          // Gemini category names, case-insensitive, e.g. "Crypto", "Commodities"
    "subcategories": [],       // Gemini subcategory slug or name
    "keywords": [],            // matched case-insensitively against title, ticker, tags/eventTags, e.g. "energy", "oil"
    "symbols": [],             // base assets, e.g. "BTC"
    "ranges": {                // every bound optional; integers
      "move_bps":        { "min": null, "max": null }, // |price move| over the agent's window (the "delta")
      "yes_price_bps":   { "min": null, "max": null }, // contract YES price
      "expiry_secs":     { "min": null, "max": null }, // time to expiry from now
      "volume_delta_24h_bps": { "min": null, "max": null } // Gemini volumeDelta24hPct
    }
  }
}
```

Empty filter lists and null bounds mean "anything". `scope: "any"` ignores all filters.

### Deterministic effects (pure functions in `aen-core`, unit-tested; never an LLM decision)

| Input | Effect |
|---|---|
| `markets` | `market_matches(filter, market) -> Result<(), Vec<MissReason>>` gates discovery: only matching markets are considered for proposals, scouting and firm mandates. Incoming challenges on non-matching markets are rejected with reason `outside_approach` |
| `ambition_bps` = a | stake target = `max_per_interaction × (2000 + 8 × a / 10) / 10000` (cautious 20 %, ambitious 100 %); default minimum conviction = `7500 − a / 4` bps (75 % → 50 %); max open interactions = `1 + 4 × a / 10000` (1 → 5). Explicit `cond.*` blocks still apply on top; `authorize()` still applies to everything |
| `collaboration_bps` = c | initiates challenges only if `c ≤ 7000`; joins firms only if `c ≥ 3000` and `firm_role = member`; may found/run a firm only if `firm_role = lead` and `c ≥ 3000`. Between 3000 and 7000 it does both |

## 2. Firms

```
Firm { id, env: "simulated", name, mandate: string (≤ 500 chars), lead_agent_id, lead_name,
       lead_share_bps (default 8000; 5000..10000), member_pool_bps (= 10000 − lead_share_bps, read-only),
       task_fee: money (guaranteed per used contribution, default "0"),
       roles: [{ role: "scout" | "analyst", slots: int (1..5), weight_bps: int }],  // weights sum to 10000
       markets: <approach markets object> | null  (null = the lead's approach markets),
       state: "open" | "closed",
       members: [{ agent_id, name, role, joined_at, contributions: int, used: int, earned: money }],
       stats: { challenges: int, settled: int, lead_realized_pnl: money, paid_to_members: money },
       actions: string[], created_at }

Contribution { id, firm_id, agent_id, agent_name, role, kind: "scout_candidate" | "analyst_signal",
               market_id, side: "above" | "below" | null, conviction_bps, interaction_id: string | null,
               used: bool, fee_paid: money, payout: money, summary, created_at }
```

### Roles (the tasks)

- **scout**: each tick, scans markets in (firm markets ∩ its own approach markets), submits at most one
  candidate (market, side, conviction from its reasoner). The lead consumes the best unused candidate
  before its own discovery.
- **analyst**: when the lead has drafted a firm proposal, each analyst evaluates it with its own
  reasoner and returns a signal (conviction for / against). The lead proposes only if the
  weight-averaged conviction meets the lead's minimum conviction; otherwise the draft is dropped.

A contribution is **used** when it fed a challenge the lead actually proposed (`interaction_id` set).

### Money (simulated ledger, balanced posting sets, through `authorize` where an agent pays)

- **Task fee (guaranteed)**: when a contribution is used, `task_fee` moves lead available → member
  available (`firm_task_fee`). Gated by `authorize` as a payment by the lead; if denied, the
  contribution stays used with `fee_paid: "0"` and a `firm_fee_denied` activity.
- **Profit share**: when a firm challenge settles with lead realized P&L `p > 0`:
  `pool = p × member_pool_bps / 10000`, split across the used contributions on that interaction by
  role weight (equal within a role), integer floor, remainder stays with the lead (`firm_payout`).
  Paid from the profit just credited, so it cannot overdraw.
- **Losses**: borne by the lead only. Members risk effort, not capital.
- No double counting: P&L stats show lead P&L net of payouts; members show `firm_income` separately
  from challenge P&L. Transfers are never profit for the payer.

### Membership

- Owner-driven: `POST /firms/{id}/join {agent_id, role}`, `/leave`.
- Autonomous: on tick, an agent with `firm_role = member` and `collaboration_bps ≥ 3000` joins an open
  firm with a free slot whose markets overlap its own (any shared category/keyword/symbol, or either
  side is `any`), preferring the highest member pool. Activity `firm_joined`.
- A lead agent can run at most one open firm. Agents can be a member of at most one firm.

## 3. API (base `/api`, existing conventions: money strings, error envelope, 422 `validation_failed`)

| Method | Path | Body → result |
|---|---|---|
| GET | `/markets/categories` | → `{ categories: string[], subcategories: {category: string[]}, structures: string[], source: "gemini" \| "fallback" }` (cached; fallback list when Gemini is down) |
| POST | `/markets/preview` | `{ markets: <approach markets>, agent_id?: string }` → `{ count: int, sample: MarketView[≤10], as_of, source }` how many live markets match right now |
| GET | `/firms` | → `Firm[]` |
| POST | `/firms` | `{ lead_agent_id, name, mandate, lead_share_bps?, task_fee?, roles?, markets? }` → **201** `Firm`. 422 if the lead is not `firm_role = lead`, already leads an open firm, or `env ≠ simulated` (`unsupported_env`) |
| GET | `/firms/{id}` | → `Firm` |
| POST | `/firms/{id}/join` | `{ agent_id, role }` → `Firm`; 409 `no_free_slot`, `already_member` |
| POST | `/firms/{id}/leave` | `{ agent_id }` → `Firm` |
| POST | `/firms/{id}/close` | `{}` → `Firm` (no new contributions; pending payouts still settle) |
| GET | `/firms/{id}/contributions` | → `Contribution[]` newest first |

`Interaction` gains `firm_id: string | null`. The block catalog (`GET /blocks` or the existing catalog
endpoint) includes `approach.profile` with its params schema. New activity kinds: `firm_created`,
`firm_joined`, `firm_left`, `firm_closed`, `firm_contribution`, `firm_task_fee`, `firm_fee_denied`,
`firm_payout`, `approach_filtered` (summary only, rate-limited).

## 4. UI

- **Approach editor** (agent builder, pinned "Approach" block; also on agent create): two wheels.
  - *Stance wheel*: angle = competitive (left) ↔ collaborative (right), radius = cautious (centre) ↔
    ambitious (edge); firm role picked from the outer ring on the collaborative half ("Join firms",
    "Lead a firm"). Shows the derived numbers (stake target, min conviction, max open deals).
  - *Market wheel* (sunburst): centre = "Any market"; inner ring = categories; outer ring =
    subcategories. Below it: structure chips, keyword chips, symbol chips, range sliders (move delta,
    YES price, expiry, volume delta). Live "N markets match now" from `/markets/preview`.
  - Every wheel has plain numeric inputs/sliders as an accessible fallback.
- **Firms**: list (lead, split, members/slots, P&L, paid to members), detail (members, roles,
  contributions with used/fee/payout, firm challenges), create form (80/20 split slider, task fee,
  roles), join/leave/close buttons driven by `actions`. Replaces "Collaborations" in the nav; the old
  two-agent agreements stay reachable from the Firms page as "Simple agreements".
