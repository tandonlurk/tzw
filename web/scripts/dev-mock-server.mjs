// DEV MOCK ONLY. Throwaway stand-in for the Rust backend, used to eyeball the UI before the real API exists.
// Implements a subset of docs/api.md with in-memory state. Never imported by the app.
// Run: npm run dev-mock   (listens on :8080 or $PORT, base path /api). Chain/vault env switches: see "MOCK chain" below.
import http from "node:http";
import { randomUUID } from "node:crypto";

const BLOCKS = [
  { type: "trigger.schedule", kind: "trigger", label: "Schedule", description: "Run on a timer", inputs: [], outputs: [{ port: "out", data_type: "event" }],
    params_schema: { type: "object", properties: { every_secs: { type: "integer", default: 60 } } } },
  { type: "reason.evaluate_opportunity", kind: "reasoning", label: "Evaluate opportunity", description: "LLM evaluation", inputs: [{ port: "in", data_type: "event" }], outputs: [{ port: "out", data_type: "opportunity" }],
    params_schema: { type: "object", properties: { style: { type: "string", enum: ["bullish", "skeptic", "neutral"], default: "neutral" }, note: { type: "string" } } } },
  { type: "cond.conviction_gte", kind: "condition", label: "Conviction >=", description: "Gate on conviction", inputs: [{ port: "in", data_type: "opportunity" }], outputs: [{ port: "pass", data_type: "opportunity" }],
    params_schema: { type: "object", properties: { bps: { type: "integer", default: 6000 }, strict: { type: "boolean" }, extra: { type: "object" } } } },
  { type: "action.propose_challenge", kind: "action", label: "Propose challenge", description: "Emit a challenge intent", inputs: [{ port: "in", data_type: "opportunity" }], outputs: [], params_schema: { type: "object", properties: {} } },
  // MOCK approach block (docs/design/approach-and-firms.md 1). Schema is a plausible stand-in for the backend's.
  { type: "approach.profile", kind: "approach", label: "Approach", description: "How the agent approaches the market: stance, firm role, markets", inputs: [], outputs: [],
    params_schema: { type: "object", properties: {
      stance: { type: "object", properties: { collaboration_bps: { type: "integer", minimum: 0, maximum: 10000, default: 3000 }, ambition_bps: { type: "integer", minimum: 0, maximum: 10000, default: 5000 } } },
      firm_role: { type: "string", enum: ["none", "member", "lead"], default: "none" },
      markets: { type: "object" } } } },
];

// ---- MOCK markets (approach filters + preview). All data is made up; source is reported as "fake".
// MOCK_MARKETS=down -> /markets/* answer 503. A preview with keyword "mockerror" also answers 503.
const MOCK_MARKETS = process.env.MOCK_MARKETS ?? "up";
const H = 3600;
const MARKETS = [
  ["BTC above 70k by Friday (mock)", "Crypto", "Bitcoin", "binary_up_down", "BTC", 420, 5500, 3 * 24 * H, 1200, ["btc", "bitcoin"]],
  ["BTC price range end of month (mock)", "Crypto", "Bitcoin", "price_ladder", "BTC", 420, 3100, 14 * 24 * H, 1200, ["btc"]],
  ["ETH above 4k this week (mock)", "Crypto", "Ethereum", "binary_up_down", "ETH", -310, 4200, 5 * 24 * H, -800, ["eth"]],
  ["SOL flips ETH volume (mock)", "Crypto", "Solana", "binary_up_down", "SOL", 870, 1800, 7 * 24 * H, 4500, ["sol"]],
  ["Which L2 leads TVL in Q1 (mock)", "Crypto", "Ethereum", "categorical", "ETH", 120, 2600, 60 * 24 * H, 300, ["l2", "eth"]],
  ["WTI crude above $80 (mock)", "Commodities", "Energy", "binary_up_down", null, 380, 4700, 10 * 24 * H, 2200, ["oil", "energy", "wti"]],
  ["Natural gas weekly close (mock)", "Commodities", "Energy", "price_ladder", null, 650, 3800, 6 * 24 * H, 900, ["gas", "energy"]],
  ["OPEC cuts output in December (mock)", "Commodities", "Energy", "binary_up_down", null, 90, 6100, 20 * 24 * H, -200, ["oil", "opec", "energy"]],
  ["Gold above $2,600 (mock)", "Commodities", "Metals", "binary_up_down", null, 150, 5200, 9 * 24 * H, 100, ["gold"]],
  ["Fed cuts at next meeting (mock)", "Economics", "Rates", "binary_up_down", null, 0, 6800, 25 * 24 * H, 600, ["fed", "rates"]],
  ["CPI print range (mock)", "Economics", "Inflation", "price_ladder", null, 0, 2900, 12 * 24 * H, 300, ["cpi", "inflation"]],
  ["Champions League winner (mock)", "Sports", "Football", "categorical", null, 0, 1400, 120 * 24 * H, 1500, ["football"]],
].map(([title, category, subcategory, structure, spot_symbol, move_bps, yes_price_bps, expiry_secs, volume_delta_24h_bps, tags]) =>
  ({ title, category, subcategory, structure, spot_symbol, move_bps, yes_price_bps, expiry_secs, volume_delta_24h_bps, tags, source: "fake" }));
const CATEGORIES = () => {
  const subcategories = {};
  for (const m of MARKETS) (subcategories[m.category] ??= []).includes(m.subcategory) || subcategories[m.category].push(m.subcategory);
  return { categories: Object.keys(subcategories), subcategories, structures: ["binary_up_down", "price_ladder", "categorical"], source: "fallback" };
};
const lc = (x) => String(x ?? "").toLowerCase();
const inBound = (v, b) => !b || ((b.min === null || b.min === undefined || v >= b.min) && (b.max === null || b.max === undefined || v <= b.max));
// Agreed semantics: categories OR subcategories; every other non-empty group ANDed.
function marketMatches(f, m) {
  if (!f || f.scope === "any") return true;
  const cats = f.categories ?? [], subs = f.subcategories ?? [];
  if ((cats.length || subs.length) && !(cats.some((c) => lc(c) === lc(m.category)) || subs.some((s) => lc(s) === lc(m.subcategory)))) return false;
  if (f.structures?.length && !f.structures.includes(m.structure)) return false;
  if (f.keywords?.length && !f.keywords.some((k) => lc(m.title).includes(lc(k)) || m.tags.some((t) => lc(t) === lc(k)))) return false;
  if (f.symbols?.length && !f.symbols.some((s) => lc(s) === lc(m.spot_symbol))) return false;
  const r = f.ranges ?? {};
  return inBound(Math.abs(m.move_bps), r.move_bps) && inBound(m.yes_price_bps, r.yes_price_bps) && inBound(m.expiry_secs, r.expiry_secs) && inBound(m.volume_delta_24h_bps, r.volume_delta_24h_bps);
}
function approachNode(graph) { return graph?.nodes?.find((n) => n.type === "approach.profile"); }
function approachErrors(graph) {
  const out = [];
  const n = approachNode(graph);
  if (!n) return out;
  if (graph.nodes.filter((x) => x.type === "approach.profile").length > 1) out.push({ code: "approach_count", message: "at most one approach block (mock)", node_id: n.id });
  for (const [k, b] of Object.entries(n.params?.markets?.ranges ?? {}))
    if (b && b.min !== null && b.max !== null && b.min > b.max) out.push({ code: "bad_param", message: `markets.ranges.${k}: min must not exceed max (mock)`, node_id: n.id, path: `graph.nodes.${n.id}.params.markets.ranges.${k}` });
  return out;
}
const graph = {
  schema_version: 1,
  nodes: [
    { id: "n1", type: "trigger.schedule", params: { every_secs: 60 } },
    { id: "n2", type: "reason.evaluate_opportunity", params: { style: "bullish" } },
    { id: "n3", type: "cond.conviction_gte", params: { bps: 6000 } },
    { id: "n4", type: "action.propose_challenge", params: {} },
  ],
  edges: [
    { from: "n1", from_port: "out", to: "n2", to_port: "in" },
    { from: "n2", from_port: "out", to: "n3", to_port: "in" },
    { from: "n3", from_port: "pass", to: "n4", to_port: "in" },
  ],
};
const TEMPLATES = [
  { id: "momentum-bull", name: "Momentum bull", description: "Backs upward moves.",
    config: { goals: "Find bullish setups", strategy: "Momentum", categories: ["crypto"], horizon_secs: 86400, risk_tolerance: "medium", schedule_secs: 60 },
    policy: { max_per_interaction: "5000000", max_aggregate_exposure: "20000000", approval_threshold: "2000000", allowed_interaction_types: ["challenge"], min_conviction_bps: 6000, stop_loss: null }, graph },
];

// ---- MOCK chain + vaults. Nothing here touches a chain: vault numbers are made up and labelled "(mock)".
// MOCK_CHAIN=disabled -> GET /chain says {enabled:false}; MOCK_CHAIN=missing -> 404 like an older backend.
// MOCK_OWNER=<address> -> owner of seeded/linked mock vaults (set it to your wallet to see owner-only actions).
// Tx hash hooks for POST /agents/{id}/vault: ending in "dead" -> 409 agent_has_open_interactions.
const MOCK_CHAIN = process.env.MOCK_CHAIN ?? "enabled";
const MOCK_OWNER = process.env.MOCK_OWNER ?? "0x1F0eb73C714Ae0BD1a97c1c8e0F34513073393Fb";
const CHAIN = {
  enabled: true, chain_id: 5042002, chain_name: "Arc Testnet", rpc_url: "https://rpc.testnet.arc.io",
  explorer_url: "https://explorer.testnet.arc.io", usdc_address: "0x3600000000000000000000000000000000000000", usdc_decimals: 6,
  escrow_address: "0x4179f2e314410D10C2906B1914bAAe8DD93b5698", vault_factory_address: "0x4EA6ac6A9F7b8fcf01ba04b177F0A098d9664C45",
  operator_address: "0x84dDb2795697e6EE319fe9bb55b5D5c1c3B90eca",
};
const vaults = {}; // agentId -> mock vault view
const chainDown = new Set(); // agentIds whose GET vault answers 503
const randAddr = () => "0x" + randomUUID().replace(/-/g, "") + randomUUID().replace(/-/g, "").slice(0, 8);
const isHash = (h) => typeof h === "string" && /^0x[0-9a-fA-F]{64}$/.test(h);
function mockVault(owner, avail, extra = {}) {
  const address = randAddr();
  return { address, owner, operator: CHAIN.operator_address, paused: false, max_per_challenge: "5000000", max_exposure: "20000000",
    onchain_balance: avail, onchain_exposure: "0", ledger_available: avail, ledger_committed: "0", drift: "0",
    explorer_url: `${CHAIN.explorer_url}/address/${address}`, ...extra };
}

const agents = [];
const wallet = { env: "simulated", unallocated: 0n, allocated: 0n, committed: 0n, entries: [] };
const activity = [];
const ledgers = {};
const entry = (account, delta, reason) => ({ id: randomUUID(), ts: new Date().toISOString(), env: "simulated", account, delta: delta.toString(), reason, interaction_id: null, tx_hash: null });
const walletOut = () => ({ ...wallet, unallocated: wallet.unallocated.toString(), allocated: wallet.allocated.toString(), committed: wallet.committed.toString() });
const agentOut = (a) => ({ ...a, balances: { available: a._avail.toString(), committed: "0" }, _avail: undefined });

// Seed three agents covering the vault states (set MOCK_SEED=0 to start empty).
function withApproach(c, a, firm_role, markets = {}) {
  return { ...graph, nodes: [{ id: "approach", type: "approach.profile", ui: { x: 0, y: -220 },
    params: { stance: { collaboration_bps: c, ambition_bps: a }, firm_role, markets: { scope: "any", structures: [], categories: [], subcategories: [], keywords: [], symbols: [],
      ranges: { move_bps: { min: null, max: null }, yes_price_bps: { min: null, max: null }, expiry_secs: { min: null, max: null }, volume_delta_24h_bps: { min: null, max: null } }, ...markets } } }, ...graph.nodes] };
}
function seed(name, env, avail, g) {
  const t = TEMPLATES[0];
  const a = { id: randomUUID(), name, description: "Seeded by the dev mock.", config: t.config, policy: t.policy, graph: g ?? t.graph, status: "active", env,
    wallet_address: null, version: 1, _avail: BigInt(avail), stats: { realized_pnl: "0", settled: 0, wins: 0 }, created_at: new Date().toISOString(), updated_at: new Date().toISOString() };
  agents.push(a);
  ledgers[a.id] = [];
  return a;
}
if (process.env.MOCK_SEED !== "0") {
  const linked = seed("Vault agent (mock)", "arc_testnet", "7500000");
  vaults[linked.id] = mockVault(MOCK_OWNER, "7500000", { onchain_balance: "9500000", onchain_exposure: "2000000", ledger_committed: "2000000", drift: "2000000" });
  linked.wallet_address = vaults[linked.id].address;
  seed("Unlinked agent (mock)", "simulated", "0");
  chainDown.add(seed("Chain down agent (mock)", "arc_testnet", "0").id);
}

// ---- MOCK firms. In-memory; payouts are made up. Set MOCK_FIRMS=0 to start with no firm.
const firms = [];
const contributions = {}; // firmId -> Contribution[]
const interactions = [];
const now = () => new Date().toISOString();
const act = (kind, summary, extra = {}) => activity.unshift({ id: randomUUID(), ts: now(), agent_id: null, kind, summary, detail: null, ...extra });
function firmOut(f) {
  const free = f.roles.some((r) => f.members.filter((m) => m.role === r.role).length < r.slots);
  const actions = f.state === "open" ? [...(free ? ["join"] : []), ...(f.members.length ? ["leave"] : []), "close"] : [];
  return { ...f, member_pool_bps: 10000 - f.lead_share_bps, actions };
}
function makeFirm(lead, body) {
  const f = { id: randomUUID(), env: "simulated", name: body.name, mandate: body.mandate ?? "", lead_agent_id: lead.id, lead_name: lead.name,
    lead_share_bps: body.lead_share_bps ?? 8000, task_fee: body.task_fee ?? "0",
    roles: body.roles ?? [{ role: "scout", slots: 2, weight_bps: 5000 }, { role: "analyst", slots: 1, weight_bps: 5000 }],
    markets: body.markets ?? null, state: "open", members: [], stats: { challenges: 0, settled: 0, lead_realized_pnl: "0", paid_to_members: "0" }, created_at: now() };
  firms.unshift(f);
  contributions[f.id] = [];
  act("firm_created", `${lead.name} opened firm ${f.name} (simulated)`, { agent_id: lead.id, detail: { firm_id: f.id } });
  return f;
}
if (process.env.MOCK_SEED !== "0") {
  const lead = seed("Lead agent (mock)", "simulated", "50000000", withApproach(6000, 7000, "lead", { scope: "specific", subcategories: ["Energy"], ranges: { move_bps: { min: 300, max: null }, yes_price_bps: { min: null, max: null }, expiry_secs: { min: null, max: null }, volume_delta_24h_bps: { min: null, max: null } } }));
  const scout = seed("Scout agent (mock)", "simulated", "5000000", withApproach(8000, 4000, "member"));
  seed("Analyst agent (mock)", "simulated", "5000000", withApproach(5000, 3000, "member"));
  seed("Second lead (mock)", "simulated", "5000000", withApproach(4000, 6000, "lead"));
  if (process.env.MOCK_FIRMS !== "0") {
    const f = makeFirm(lead, { name: "Energy desk (mock)", mandate: "Trade energy prediction markets with a >3% move.", task_fee: "250000" });
    f.members.push({ agent_id: scout.id, name: scout.name, role: "scout", joined_at: now(), contributions: 3, used: 1, earned: "650000" });
    act("firm_joined", `${scout.name} joined ${f.name} as scout (simulated)`, { agent_id: scout.id, detail: { firm_id: f.id, role: "scout" } });
    const ix = { id: randomUUID(), kind: "challenge", env: "simulated", state: "settled", proposer_id: lead.id, proposer_name: lead.name,
      counterparty_id: agents[0].id, counterparty_name: agents[0].name,
      terms: { market: { kind: "gemini_spot", symbol: "WTIUSD" }, direction: "above", threshold: 80000000, start_ts: 0, end_ts: 0, stake: "2000000", resolution_source: "gemini_spot_ticker", formula: "spot_last_price_micros_v1" },
      stake: "2000000", market_title: "WTI crude above $80 (mock)", market_source: "fake", claim: "WTI closes above $80 (mock)", turn: null, rounds: 1, max_rounds: 3,
      pending_approvals: 0, outcome: "a", winner_id: lead.id, observed: null, observed_display: null, observed_at: now(), status_reason: null, firm_id: f.id,
      resolution: "backend_administered", start_at: now(), end_at: now(), created_at: now(), updated_at: now() };
    interactions.push(ix);
    contributions[f.id] = [
      { id: randomUUID(), firm_id: f.id, agent_id: scout.id, agent_name: scout.name, role: "scout", kind: "scout_candidate", market_id: "ng-weekly", side: "below", conviction_bps: 5800, interaction_id: null, used: false, fee_paid: "0", payout: "0", summary: "Natural gas weekly close looks rich (mock)", created_at: now() },
      { id: randomUUID(), firm_id: f.id, agent_id: scout.id, agent_name: scout.name, role: "scout", kind: "scout_candidate", market_id: "wti-80", side: "above", conviction_bps: 7100, interaction_id: ix.id, used: true, fee_paid: "250000", payout: "400000", summary: "WTI momentum above $80 (mock)", created_at: now() },
      { id: randomUUID(), firm_id: f.id, agent_id: scout.id, agent_name: scout.name, role: "scout", kind: "scout_candidate", market_id: "opec-dec", side: null, conviction_bps: 5000, interaction_id: null, used: false, fee_paid: "0", payout: "0", summary: "OPEC cut is priced in (mock)", created_at: now() },
    ];
    f.stats = { challenges: 1, settled: 1, lead_realized_pnl: "1600000", paid_to_members: "650000" };
    act("firm_task_fee", `${lead.name} paid ${scout.name} a 0.25 USDC task fee (simulated)`, { agent_id: lead.id, detail: { firm_id: f.id, amount: "250000" } });
    act("firm_payout", `${scout.name} earned 0.40 USDC from ${f.name} (simulated)`, { agent_id: scout.id, interaction_id: ix.id, detail: { firm_id: f.id, amount: "400000" } });
    act("approach_filtered", "Scout agent (mock) skipped 9 markets outside its approach", { agent_id: scout.id, detail: { skipped: 9 } });
  }
}

function validateInput(i) {
  const details = [];
  if (!i.name) details.push({ field: "name", message: "name must not be empty" });
  if (BigInt(i.policy.max_per_interaction) > BigInt(i.policy.max_aggregate_exposure))
    details.push({ field: "policy.max_per_interaction", message: "must not exceed max_aggregate_exposure" });
  for (const e of approachErrors(i.graph)) details.push({ code: e.code, path: e.path ?? `graph.nodes.${e.node_id}`, message: e.message });
  return details;
}

const send = (res, code, body) => {
  res.writeHead(code, { "Content-Type": "application/json", "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "content-type", "Access-Control-Allow-Methods": "GET,POST,PUT,OPTIONS" });
  res.end(body === undefined ? "" : JSON.stringify(body));
};
const err = (res, code, c, message, details = []) => send(res, code, { error: { code: c, message, details } });

http.createServer(async (req, res) => {
  if (req.method === "OPTIONS") return send(res, 204);
  const url = new URL(req.url, "http://x");
  const p = url.pathname.replace(/^\/api/, "");
  let body = {};
  if (req.method !== "GET") {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    try { body = JSON.parse(Buffer.concat(chunks).toString() || "{}"); } catch { return err(res, 400, "bad_json", "invalid JSON"); }
  }
  let m;
  if (p === "/health") return send(res, 200, { status: "ok" });
  if (p === "/templates") return send(res, 200, TEMPLATES);
  if (p === "/graph/blocks") return send(res, 200, BLOCKS);
  if (p === "/graph/validate" && req.method === "POST") {
    const errors = [];
    const triggers = body.nodes.filter((n) => n.type.startsWith("trigger."));
    if (triggers.length !== 1) errors.push({ code: "trigger_count", message: "exactly one trigger required", node_id: null });
    for (const n of body.nodes) if (n.type === "cond.conviction_gte" && n.params.bps > 10000) errors.push({ code: "bad_param", message: "bps must be <= 10000", node_id: n.id });
    for (const e of approachErrors(body)) errors.push({ code: e.code, message: e.message, node_id: e.node_id });
    return send(res, 200, { valid: errors.length === 0, errors });
  }
  if (p.startsWith("/markets/")) {
    if (MOCK_MARKETS === "down") return err(res, 503, "market_data_unavailable", "market data is down (mock)");
    if (p === "/markets/categories" && req.method === "GET") return send(res, 200, CATEGORIES());
    if (p === "/markets/preview" && req.method === "POST") {
      if ((body.markets?.keywords ?? []).some((k) => lc(k) === "mockerror")) return err(res, 503, "market_data_unavailable", "preview failed (mock hook: keyword mockerror)");
      for (const [k, b] of Object.entries(body.markets?.ranges ?? {}))
        if (b && b.min !== null && b.max !== null && b.min > b.max) return err(res, 422, "validation_failed", "invalid market filter", [{ path: `markets.ranges.${k}`, message: "min must not exceed max (mock)" }]);
      const hits = MARKETS.filter((m) => marketMatches(body.markets, m));
      await new Promise((r) => setTimeout(r, 250));
      return send(res, 200, { count: hits.length, sample: hits.slice(0, 10).map(({ tags, ...m }) => m), as_of: now(), source: "fake" });
    }
  }
  if (p === "/firms" && req.method === "GET") return send(res, 200, firms.map(firmOut));
  if (p === "/firms" && req.method === "POST") {
    const lead = agents.find((a) => a.id === body.lead_agent_id);
    if (!lead) return err(res, 422, "validation_failed", "invalid firm", [{ path: "lead_agent_id", message: "unknown agent (mock)" }]);
    const details = [];
    if (approachNode(lead.graph)?.params?.firm_role !== "lead") details.push({ path: "lead_agent_id", message: "agent's approach firm_role is not lead (mock)" });
    if (lead.env !== "simulated") return err(res, 422, "unsupported_env", "firms are simulated-only (mock)");
    if (firms.some((f) => f.state === "open" && f.lead_agent_id === lead.id)) details.push({ path: "lead_agent_id", message: "agent already leads an open firm (mock)" });
    if (!body.name?.trim()) details.push({ path: "name", message: "name must not be empty (mock)" });
    if ((body.mandate ?? "").length > 500) details.push({ path: "mandate", message: "at most 500 characters (mock)" });
    if (body.lead_share_bps !== undefined && (body.lead_share_bps < 5000 || body.lead_share_bps > 10000)) details.push({ path: "lead_share_bps", message: "5000..10000 (mock)" });
    if (body.roles && body.roles.reduce((s, r) => s + r.weight_bps, 0) !== 10000) details.push({ path: "roles", message: "weights must sum to 10000 (mock)" });
    if (details.length) return err(res, 422, "validation_failed", "invalid firm", details);
    return send(res, 201, firmOut(makeFirm(lead, body)));
  }
  if ((m = p.match(/^\/firms\/([^/]+)(?:\/(join|leave|close|contributions))?$/))) {
    const f = firms.find((x) => x.id === m[1]);
    if (!f) return err(res, 404, "not_found", "firm not found (mock)");
    const sub = m[2];
    if (!sub && req.method === "GET") return send(res, 200, firmOut(f));
    if (sub === "contributions") return send(res, 200, contributions[f.id] ?? []);
    if (f.state !== "open") return err(res, 409, "firm_closed", "firm is closed (mock)");
    if (sub === "close") { f.state = "closed"; act("firm_closed", `${f.name} closed (simulated)`, { agent_id: f.lead_agent_id, detail: { firm_id: f.id } }); return send(res, 200, firmOut(f)); }
    const a = agents.find((x) => x.id === body.agent_id);
    if (!a) return err(res, 422, "validation_failed", "invalid request", [{ path: "agent_id", message: "unknown agent (mock)" }]);
    if (sub === "join") {
      if (f.members.some((x) => x.agent_id === a.id) || firms.some((o) => o.state === "open" && o.members.some((x) => x.agent_id === a.id))) return err(res, 409, "already_member", "agent is already a member of a firm (mock)");
      if (a.id === f.lead_agent_id) return err(res, 422, "validation_failed", "invalid request", [{ path: "agent_id", message: "the lead cannot join its own firm (mock)" }]);
      const role = f.roles.find((r) => r.role === body.role);
      if (!role) return err(res, 422, "validation_failed", "invalid request", [{ path: "role", message: "firm has no such role (mock)" }]);
      if (f.members.filter((x) => x.role === role.role).length >= role.slots) return err(res, 409, "no_free_slot", `no free ${role.role} slot (mock)`);
      f.members.push({ agent_id: a.id, name: a.name, role: role.role, joined_at: now(), contributions: 0, used: 0, earned: "0" });
      act("firm_joined", `${a.name} joined ${f.name} as ${role.role} (simulated)`, { agent_id: a.id, detail: { firm_id: f.id, role: role.role } });
      return send(res, 200, firmOut(f));
    }
    if (sub === "leave") {
      if (!f.members.some((x) => x.agent_id === a.id)) return err(res, 409, "not_member", "agent is not a member (mock)");
      f.members = f.members.filter((x) => x.agent_id !== a.id);
      act("firm_left", `${a.name} left ${f.name} (simulated)`, { agent_id: a.id, detail: { firm_id: f.id } });
      return send(res, 200, firmOut(f));
    }
  }
  if (p === "/wallet") return send(res, 200, walletOut());
  if (p === "/wallet/simulated-deposit") {
    const amt = BigInt(body.amount);
    wallet.unallocated += amt;
    wallet.entries.unshift(entry("user:unallocated", amt, "simulated deposit"));
    activity.unshift({ id: randomUUID(), ts: new Date().toISOString(), agent_id: null, kind: "wallet", summary: "Simulated funds added", detail: { amount: body.amount } });
    return send(res, 200, walletOut());
  }
  if (p === "/activity") return send(res, 200, activity);
  if (p === "/agents" && req.method === "GET") return send(res, 200, agents.map(agentOut));
  if (p === "/agents" && req.method === "POST") {
    const details = validateInput(body);
    if (details.length) return err(res, 422, "validation_failed", "invalid agent", details);
    const a = { id: randomUUID(), ...body, status: "active", env: "simulated", wallet_address: null, version: 1, _avail: 0n, stats: { realized_pnl: "0", settled: 0, wins: 0 }, created_at: new Date().toISOString(), updated_at: new Date().toISOString() };
    agents.push(a);
    ledgers[a.id] = [];
    activity.unshift({ id: randomUUID(), ts: a.created_at, agent_id: a.id, kind: "agent_created", summary: `Agent ${a.name} created`, detail: { version: 1 } });
    return send(res, 201, agentOut(a));
  }
  if (p === "/interactions" && req.method === "GET") {
    const aid = url.searchParams.get("agent_id");
    return send(res, 200, interactions.filter((i) => !aid || i.proposer_id === aid || i.counterparty_id === aid));
  }
  if (p === "/collaborations" && req.method === "GET") return send(res, 200, []);
  if (p === "/chain" && req.method === "GET") {
    if (MOCK_CHAIN === "missing") return err(res, 404, "not_found", `no route ${req.method} ${p}`);
    return send(res, 200, MOCK_CHAIN === "disabled" ? { enabled: false } : CHAIN);
  }
  if ((m = p.match(/^\/agents\/([^/]+)\/vault(\/sync)?$/))) {
    const a = agents.find((x) => x.id === m[1]);
    if (!a) return err(res, 404, "not_found", "agent not found");
    if (MOCK_CHAIN !== "enabled") return err(res, 503, "chain_unavailable", "chain is not configured (mock)");
    if (chainDown.has(a.id)) return err(res, 503, "chain_unavailable", "RPC request failed (mock)");
    const v = vaults[a.id];
    if (!m[2] && req.method === "GET") return v ? send(res, 200, v) : err(res, 404, "vault_not_linked", "this agent has no vault (mock)");
    if (!m[2] && req.method === "POST") {
      if (!isHash(body.tx_hash)) return err(res, 422, "invalid_vault_tx", "tx_hash must be 0x + 64 hex (mock)");
      if (v) return err(res, 409, "vault_already_linked", "agent already has a vault (mock)");
      if (body.tx_hash.toLowerCase().endsWith("dead")) return err(res, 409, "agent_has_open_interactions", "agent has open interactions (mock)");
      vaults[a.id] = mockVault(MOCK_OWNER, "0");
      a.env = "arc_testnet";
      a.wallet_address = vaults[a.id].address;
      a._avail = 0n;
      return send(res, 200, vaults[a.id]);
    }
    if (m[2] && req.method === "POST") {
      if (!v) return err(res, 404, "vault_not_linked", "this agent has no vault (mock)");
      if (!isHash(body.tx_hash)) return err(res, 422, "invalid_vault_tx", "tx_hash must be 0x + 64 hex (mock)");
      return send(res, 200, { credited: [], vault: v }); // mock cannot read the chain, so it never credits anything
    }
  }
  if ((m = p.match(/^\/agents\/([^/]+)(?:\/(status|ledger|allocate|deallocate|versions))?$/))) {
    const a = agents.find((x) => x.id === m[1]);
    if (!a) return err(res, 404, "not_found", "agent not found");
    const sub = m[2];
    if (!sub && req.method === "GET") return send(res, 200, agentOut(a));
    if (!sub && req.method === "PUT") {
      const details = validateInput(body);
      if (details.length) return err(res, 422, "validation_failed", "invalid agent", details);
      Object.assign(a, body, { version: a.version + 1, updated_at: new Date().toISOString() });
      return send(res, 200, agentOut(a));
    }
    if (sub === "status") { a.status = body.status; return send(res, 200, agentOut(a)); }
    if (sub === "ledger") return send(res, 200, ledgers[a.id]);
    if (sub === "versions") return send(res, 200, []);
    if (sub === "allocate" || sub === "deallocate") {
      const amt = BigInt(body.amount);
      if (sub === "allocate") {
        if (amt > wallet.unallocated) return err(res, 422, "insufficient_funds", "not enough unallocated funds");
        wallet.unallocated -= amt; wallet.allocated += amt; a._avail += amt;
      } else {
        if (amt > a._avail) return err(res, 422, "insufficient_funds", "amount exceeds agent available balance");
        wallet.unallocated += amt; wallet.allocated -= amt; a._avail -= amt;
      }
      const d = sub === "allocate" ? amt : -amt;
      ledgers[a.id].unshift(entry(`agent:${a.id}:available`, d, sub));
      wallet.entries.unshift(entry("user:unallocated", -d, sub));
      return send(res, 200, agentOut(a));
    }
  }
  err(res, 404, "not_found", `no route ${req.method} ${p}`);
}).listen(Number(process.env.PORT ?? 8080), () => console.log(`DEV MOCK listening on http://localhost:${process.env.PORT ?? 8080}/api (chain: ${MOCK_CHAIN})`));
