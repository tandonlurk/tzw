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
];
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
function seed(name, env, avail) {
  const t = TEMPLATES[0];
  const a = { id: randomUUID(), name, description: "Seeded by the dev mock.", config: t.config, policy: t.policy, graph: t.graph, status: "active", env,
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

function validateInput(i) {
  const details = [];
  if (!i.name) details.push({ field: "name", message: "name must not be empty" });
  if (BigInt(i.policy.max_per_interaction) > BigInt(i.policy.max_aggregate_exposure))
    details.push({ field: "policy.max_per_interaction", message: "must not exceed max_aggregate_exposure" });
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
    return send(res, 200, { valid: errors.length === 0, errors });
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
  if ((p === "/interactions" || p === "/collaborations") && req.method === "GET") return send(res, 200, []);
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
