import type {
  Env,
  ActivityItem,
  ApprovalDecision,
  Approval,
  Interaction,
  InteractionDetail,
  Portfolio,
  TickReport,
  Agent,
  CollabAction,
  CollabDetail,
  CollabInput,
  Collaboration,
  AgentInput,
  AgentStatus,
  AgentVersion,
  BlockDef,
  Graph,
  GraphValidation,
  LedgerEntry,
  Money,
  Suggestion,
  SuggestionStatus,
  GenerateSuggestionsResult,
  Template,
  TestRun,
  Wallet,
  MultiplyInput,
  MultiplyReport,
  ChainConfig,
  VaultView,
  VaultSyncResult,
  ApproachMarkets,
  Contribution,
  Firm,
  FirmInput,
  FirmRoleName,
  MarketCategories,
  MarketPreview,
} from "./types";

export const API_URL = (process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080/api").replace(/\/+$/, "");

/** One validation/detail item from the server. Shape is not fixed by the contract, so keep it loose. */
export type ErrorDetail = unknown;

/**
 * Uniform error. `unreachable` is true when the backend could not be contacted at all
 * (network down, CORS, DNS); in that case `status` is 0.
 */
export class ApiError extends Error {
  constructor(
    public readonly code: string,
    message: string,
    public readonly status: number,
    public readonly details: ErrorDetail[] = [],
    public readonly unreachable = false,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

export function isApiError(e: unknown): e is ApiError {
  return e instanceof ApiError;
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`${API_URL}${path}`, {
      method,
      headers: body === undefined ? undefined : { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
      cache: "no-store",
    });
  } catch {
    throw new ApiError("unreachable", `Cannot reach the backend at ${API_URL}`, 0, [], true);
  }

  const text = await res.text();
  let json: unknown = undefined;
  if (text) {
    try {
      json = JSON.parse(text);
    } catch {
      json = undefined;
    }
  }

  if (!res.ok) {
    const err = (json as { error?: { code?: string; message?: string; details?: unknown[] } } | undefined)?.error;
    // The same-origin proxy (/backend, D-70) answers 502 when it cannot reach the backend: same as unreachable.
    if (res.status === 502 && err?.code === "upstream_unreachable") {
      throw new ApiError("unreachable", `Cannot reach the backend through ${API_URL}`, 0, [], true);
    }
    throw new ApiError(
      err?.code ?? `http_${res.status}`,
      err?.message ?? `Request failed with status ${res.status}`,
      res.status,
      Array.isArray(err?.details) ? err.details : [],
    );
  }
  return json as T;
}

const get = <T>(path: string) => request<T>("GET", path);
const post = <T>(path: string, body: unknown) => request<T>("POST", path, body);

export const api = {
  health: () => get<{ status: "ok" }>("/health"),

  listAgents: () => get<Agent[]>("/agents"),
  createAgent: (input: AgentInput) => post<Agent>("/agents", input),
  getAgent: (id: string) => get<Agent>(`/agents/${encodeURIComponent(id)}`),
  updateAgent: (id: string, input: AgentInput) => request<Agent>("PUT", `/agents/${encodeURIComponent(id)}`, input),
  setAgentStatus: (id: string, status: AgentStatus) =>
    post<Agent>(`/agents/${encodeURIComponent(id)}/status`, { status }),
  listAgentVersions: (id: string) => get<AgentVersion[]>(`/agents/${encodeURIComponent(id)}/versions`),
  getAgentLedger: (id: string) => get<LedgerEntry[]>(`/agents/${encodeURIComponent(id)}/ledger`),
  allocate: (id: string, amount: Money) => post<Agent>(`/agents/${encodeURIComponent(id)}/allocate`, { amount }),
  deallocate: (id: string, amount: Money) => post<Agent>(`/agents/${encodeURIComponent(id)}/deallocate`, { amount }),
  multiplyAgent: (id: string, input: MultiplyInput) => post<MultiplyReport>(`/agents/${encodeURIComponent(id)}/multiply`, input),

  listTemplates: () => get<Template[]>("/templates"),
  listBlocks: () => get<BlockDef[]>("/graph/blocks"),
  validateGraph: (graph: Graph) => post<GraphValidation>("/graph/validate", graph),

  getWallet: () => get<Wallet>("/wallet"),
  simulatedDeposit: (amount: Money) => post<Wallet>("/wallet/simulated-deposit", { amount }),

  listActivity: (opts: { agent_id?: string; interaction_id?: string; collaboration_id?: string; limit?: number } = {}) => {
    const q = new URLSearchParams();
    if (opts.collaboration_id) q.set("collaboration_id", opts.collaboration_id);
    if (opts.interaction_id) q.set("interaction_id", opts.interaction_id);
    if (opts.agent_id) q.set("agent_id", opts.agent_id);
    if (opts.limit) q.set("limit", String(opts.limit));
    const qs = q.toString();
    return get<ActivityItem[]>(`/activity${qs ? `?${qs}` : ""}`);
  },

  listInteractions: (opts: { agent_id?: string; state?: string; limit?: number } = {}) => {
    const q = new URLSearchParams();
    for (const [k, v] of Object.entries(opts)) if (v !== undefined && v !== "") q.set(k, String(v));
    const qs = q.toString();
    return get<Interaction[]>(`/interactions${qs ? `?${qs}` : ""}`);
  },
  getInteraction: (id: string) => get<InteractionDetail>(`/interactions/${encodeURIComponent(id)}`),
  listApprovals: (status = "pending") => get<Approval[]>(`/approvals?status=${encodeURIComponent(status)}`),
  approve: (id: string) => post<ApprovalDecision>(`/approvals/${encodeURIComponent(id)}/approve`, {}),
  reject: (id: string) => post<ApprovalDecision>(`/approvals/${encodeURIComponent(id)}/reject`, {}),

  listSuggestions: (opts: { status?: SuggestionStatus; agent_id?: string } = {}) => {
    const q = new URLSearchParams();
    if (opts.status) q.set("status", opts.status);
    if (opts.agent_id) q.set("agent_id", opts.agent_id);
    const qs = q.toString();
    return get<Suggestion[]>(`/suggestions${qs ? `?${qs}` : ""}`);
  },
  generateSuggestions: () => post<GenerateSuggestionsResult>("/suggestions/generate", {}),
  acceptSuggestion: (id: string) => post<Suggestion>(`/suggestions/${encodeURIComponent(id)}/accept`, {}),
  dismissSuggestion: (id: string) => post<Suggestion>(`/suggestions/${encodeURIComponent(id)}/dismiss`, {}),
  listCollaborations: (opts: { agent_id?: string; state?: string } = {}) => {
    const q = new URLSearchParams();
    for (const [k, v] of Object.entries(opts)) if (v) q.set(k, v);
    const qs = q.toString();
    return get<Collaboration[]>(`/collaborations${qs ? `?${qs}` : ""}`);
  },
  getCollaboration: (id: string) => get<CollabDetail>(`/collaborations/${encodeURIComponent(id)}`),
  createCollaboration: (input: CollabInput) => post<Collaboration>("/collaborations", input),
  collabAction: (id: string, action: Exclude<CollabAction, "fund" | "cancel">) =>
    post<Collaboration>(`/collaborations/${encodeURIComponent(id)}/${action}`, {}),
  fundCollaboration: (id: string, ownerApproved = false) =>
    post<Collaboration>(`/collaborations/${encodeURIComponent(id)}/fund`, ownerApproved ? { owner_approved: true } : {}),
  cancelCollaboration: (id: string, reason?: string) =>
    post<Collaboration>(`/collaborations/${encodeURIComponent(id)}/cancel`, reason ? { reason } : {}),
  testRunCode: (agentId: string, source?: string) =>
    post<TestRun>(`/agents/${encodeURIComponent(agentId)}/code/test-run`, source === undefined ? {} : { source }),
  /** One environment per response; simulated and testnet funds are never summed. */
  getPortfolio: (env: Env = "simulated") => get<Portfolio>(`/stats/portfolio?env=${env}`),

  getChain: () => get<ChainConfig>("/chain"),
  getVault: (agentId: string) => get<VaultView>(`/agents/${encodeURIComponent(agentId)}/vault`),
  /** Link the vault created by `txHash` (a VaultFactory.createVault tx) to the agent. */
  linkVault: (agentId: string, txHash: string) =>
    post<VaultView>(`/agents/${encodeURIComponent(agentId)}/vault`, { tx_hash: txHash }),
  /** Record USDC moved into or out of the agent's vault by `txHash` in the ledger. Idempotent per tx. */
  syncVault: (agentId: string, txHash: string) =>
    post<VaultSyncResult>(`/agents/${encodeURIComponent(agentId)}/vault/sync`, { tx_hash: txHash }),
  devTick: () => post<TickReport>("/dev/tick", {}),

  getMarketCategories: () => get<MarketCategories>("/markets/categories"),
  previewMarkets: (markets: ApproachMarkets, agentId?: string) =>
    post<MarketPreview>("/markets/preview", agentId ? { markets, agent_id: agentId } : { markets }),
  listFirms: () => get<Firm[]>("/firms"),
  getFirm: (id: string) => get<Firm>(`/firms/${encodeURIComponent(id)}`),
  createFirm: (input: FirmInput) => post<Firm>("/firms", input),
  joinFirm: (id: string, agentId: string, role: FirmRoleName) =>
    post<Firm>(`/firms/${encodeURIComponent(id)}/join`, { agent_id: agentId, role }),
  leaveFirm: (id: string, agentId: string) => post<Firm>(`/firms/${encodeURIComponent(id)}/leave`, { agent_id: agentId }),
  closeFirm: (id: string) => post<Firm>(`/firms/${encodeURIComponent(id)}/close`, {}),
  listFirmContributions: (id: string) => get<Contribution[]>(`/firms/${encodeURIComponent(id)}/contributions`),
};
