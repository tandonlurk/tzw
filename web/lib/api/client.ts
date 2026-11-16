import type {
  ActivityItem,
  ApprovalDecision,
  Approval,
  Interaction,
  InteractionDetail,
  Portfolio,
  TickReport,
  Agent,
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
  Wallet,
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

  listTemplates: () => get<Template[]>("/templates"),
  listBlocks: () => get<BlockDef[]>("/graph/blocks"),
  validateGraph: (graph: Graph) => post<GraphValidation>("/graph/validate", graph),

  getWallet: () => get<Wallet>("/wallet"),
  simulatedDeposit: (amount: Money) => post<Wallet>("/wallet/simulated-deposit", { amount }),

  listActivity: (opts: { agent_id?: string; interaction_id?: string; limit?: number } = {}) => {
    const q = new URLSearchParams();
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
  getPortfolio: () => get<Portfolio>("/stats/portfolio"),
  devTick: () => post<TickReport>("/dev/tick", {}),
};
