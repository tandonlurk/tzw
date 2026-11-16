// Types mirroring docs/api.md (v0). Money is a string of integer USDC minor units (6 decimals).

export type Money = string;
export type Env = "arc_testnet" | "simulated";
export type AgentStatus = "active" | "paused";
export type RiskTolerance = "low" | "medium" | "high";
export type InteractionType = "challenge" | "collaboration";

export interface AgentConfig {
  goals: string;
  strategy: string;
  categories: string[];
  horizon_secs: number;
  risk_tolerance: RiskTolerance;
  schedule_secs: number;
}

export interface AgentPolicy {
  max_per_interaction: Money;
  max_aggregate_exposure: Money;
  approval_threshold: Money;
  allowed_interaction_types: InteractionType[];
  min_conviction_bps: number;
  stop_loss: Money | null;
}

export interface GraphNode {
  id: string;
  type: string;
  params: Record<string, unknown>;
  /** Optional canvas position, persisted by the backend. */
  ui?: { x: number; y: number };
}

export interface GraphEdge {
  from: string;
  from_port: string;
  to: string;
  to_port: string;
}

export interface Graph {
  schema_version: 1;
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export interface Balances {
  available: Money;
  committed: Money;
}

export interface Stats {
  realized_pnl: Money; // signed
  settled: number;
  wins: number;
}

export interface Agent {
  id: string;
  name: string;
  description: string;
  status: AgentStatus;
  env: Env;
  wallet_address: string | null;
  version: number;
  config: AgentConfig;
  policy: AgentPolicy;
  graph: Graph;
  balances: Balances;
  stats: Stats;
  created_at: string;
  updated_at: string;
}

export interface AgentInput {
  name: string;
  description: string;
  config: AgentConfig;
  policy: AgentPolicy;
  graph: Graph;
}

export interface Template {
  id: string;
  name: string;
  description: string;
  config: AgentConfig;
  policy: AgentPolicy;
  graph: Graph;
}

export interface AgentVersion {
  version: number;
  created_at: string;
  config: AgentConfig;
  policy: AgentPolicy;
  graph: Graph;
}

export type BlockKind = "trigger" | "reasoning" | "condition" | "action";

export interface PortDef {
  port: string;
  data_type: string;
}

export interface JsonSchema {
  type?: string;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  enum?: unknown[];
  default?: unknown;
  title?: string;
  description?: string;
  minimum?: number;
  maximum?: number;
  [key: string]: unknown;
}

export interface BlockDef {
  type: string;
  kind: BlockKind;
  label: string;
  description: string;
  inputs: PortDef[];
  outputs: PortDef[];
  params_schema: JsonSchema;
}

export interface GraphError {
  code: string;
  message: string;
  node_id: string | null;
}

export interface GraphValidation {
  valid: boolean;
  errors: GraphError[];
}

export interface LedgerEntry {
  id: string;
  ts: string;
  env: Env;
  account: string;
  delta: Money; // signed
  reason: string;
  interaction_id: string | null;
  tx_hash: string | null;
}

export interface ActivityItem {
  id: string;
  ts: string;
  agent_id: string | null;
  interaction_id?: string | null;
  collaboration_id?: string | null;
  kind: string;
  summary: string;
  detail: Record<string, unknown> | null;
}

export interface Wallet {
  env: Env;
  unallocated: Money;
  allocated: Money;
  committed: Money;
  entries: LedgerEntry[];
}

// ---- M2: challenges (docs/api.md "M2: challenges") ----

export type MarketRef =
  | { kind: "gemini_prediction_contract"; event_ticker: string; instrument_symbol: string }
  | { kind: "gemini_spot"; symbol: string };

export interface ChallengeTerms {
  market: MarketRef;
  direction: "above" | "below"; // the proposer's side
  threshold: number;
  start_ts: number;
  end_ts: number;
  stake: Money; // per side
  resolution_source: "gemini_prediction_markets" | "gemini_spot_ticker";
  formula: "pm_yes_price_bps_v1" | "spot_last_price_micros_v1";
}

export type InteractionState =
  | "proposed" | "negotiating" | "accepted" | "awaiting_approval" | "funded" | "active"
  | "resolving" | "settled" | "cancelled" | "disputed" | "failed";

export interface Interaction {
  id: string;
  kind: "challenge";
  env: Env;
  state: InteractionState;
  proposer_id: string;
  proposer_name: string;
  counterparty_id: string;
  counterparty_name: string;
  terms: ChallengeTerms;
  stake: Money;
  market_title: string;
  market_source: "gemini" | "fake" | null;
  claim: string;
  turn: string | null;
  rounds: number;
  max_rounds: number;
  pending_approvals: number;
  outcome: "a" | "b" | "void" | null;
  winner_id: string | null;
  observed: number | null;
  observed_display: string | null;
  observed_at: string | null;
  status_reason: string | null;
  resolution: "backend_administered";
  start_at: string;
  end_at: string;
  created_at: string;
  updated_at: string;
}

export interface NegotiationRound {
  seq: number;
  agent_id: string;
  action: "propose" | "counter" | "accept" | "reject";
  terms: ChallengeTerms | null;
  summary: string;
  ts: string;
}

export interface RunIntent {
  intent: { type: string; stake?: Money };
  terms: ChallengeTerms | null;
  ask_owner: boolean;
  summary: string;
}

export interface Run {
  id: string;
  ts: string;
  agent_id: string;
  agent_version: number;
  trigger: string;
  status: "ok" | "no_action" | "error";
  error: string | null;
  reasoner: string;
  trace: { node_id: string; node_type: string; summary: string }[];
  intents: RunIntent[];
}

export type ApprovalStatus = "pending" | "approved" | "rejected" | "expired" | "superseded";

export interface Approval {
  id: string;
  interaction_id: string;
  agent_id: string;
  agent_name: string;
  stake: Money;
  status: ApprovalStatus;
  reason: string;
  requested_at: string;
  decided_at: string | null;
  expires_at: string;
  market_title: string;
  env: Env;
}

export interface InteractionDetail extends Interaction {
  negotiation: NegotiationRound[];
  runs: Run[];
  approvals: Approval[];
  ledger: LedgerEntry[];
  activity: ActivityItem[];
}

export interface ApprovalDecision {
  approval: Approval;
  interaction: Interaction;
}

export interface TickReport {
  now: string;
  proposed: number;
  denied: number;
  countered: number;
  accepted: number;
  rejected: number;
  approvals_requested: number;
  funded: number;
  resolving: number;
  settled: number;
  voided: number;
  cancelled: number;
  errors: string[];
}

export interface Portfolio {
  env: Env;
  as_of: string;
  totals: {
    unallocated: Money;
    available: Money;
    committed: Money;
    allocated: Money;
    at_risk: Money;
    realized_pnl: Money;
    unrealized_pnl: Money | null;
  };
  pnl_over_time: {
    ts: string;
    interaction_id: string;
    agent_id: string;
    delta: Money;
    agent_cumulative: Money;
    portfolio_cumulative: Money;
  }[];
  allocation: { agent_id: string; name: string; available: Money; committed: Money }[];
  agents: {
    agent_id: string;
    name: string;
    realized_pnl: Money;
    settled: number;
    wins: number;
    losses: number;
    win_rate_bps: number | null;
  }[];
  outcomes: {
    settled: number;
    voided: number;
    cancelled: number;
    failed: number;
    open: number;
    active: number;
    by_state: Record<string, number>;
  };
  formulas: Record<string, string>;
}

export type SuggestionStatus = "pending" | "accepted" | "dismissed" | "expired";
export type SuggestionKind =
  | "approval_expiring"
  | "idle_funds"
  | "lower_max_stake"
  | "repeated_denials"
  | "raise_approval_threshold"
  | "agent_halted"
  | "general";

/** Only the fields a suggestion would change; nothing else can be named. */
export interface ProposedChange {
  policy?: {
    max_per_interaction?: Money;
    max_aggregate_exposure?: Money;
    approval_threshold?: Money;
    stop_loss?: Money;
    min_conviction_bps?: number;
  };
  config?: { risk_tolerance?: "low" | "medium" | "high" };
}

export interface Suggestion {
  id: string;
  agent_id: string | null;
  agent_name: string | null;
  kind: SuggestionKind;
  title: string;
  body: string;
  proposed_change: ProposedChange | null;
  evidence: Record<string, unknown>;
  loosens_control: boolean;
  source: "rules" | "grok";
  status: SuggestionStatus;
  created_at: string;
  decided_at: string | null;
}

export interface GenerateSuggestionsResult {
  source: "rules" | "grok";
  created: Suggestion[];
}

// ---- M3: collaborations (docs/api.md "Collaborations") ----

export type CollabState = "proposed" | "accepted" | "rejected" | "funded" | "completed" | "cancelled";
export type CollabAction = "accept" | "reject" | "fund" | "complete" | "cancel";

export interface CollabParty {
  agent_id: string;
  name: string;
  contribution: Money;
  share_bps: number;
  expected_payout: Money;
  payout: Money | null;
  net: Money | null; // signed, once completed
}

export interface Collaboration {
  id: string;
  env: Env;
  state: CollabState;
  objective: string;
  proposer: CollabParty;
  partner: CollabParty;
  pool: Money;
  resolution: "owner_declared";
  status_reason: string | null;
  actions: CollabAction[];
  created_at: string;
  updated_at: string;
  funded_at: string | null;
  closed_at: string | null;
}

export interface CollabDetail {
  collaboration: Collaboration;
  ledger: LedgerEntry[];
  activity: ActivityItem[];
}

export interface CollabInput {
  proposer_id: string;
  partner_id: string;
  objective: string;
  proposer_share_bps: number;
  proposer_contribution: Money;
  partner_contribution: Money;
}

// ---- Custom code (docs/api.md "Custom code") ----

export interface TestRunIntent {
  intent: { type: string; stake?: Money };
  decision: { result: "allow" | "require_approval" | "deny"; reasons: string[] };
}

export interface TestRun {
  ok: boolean;
  error: { code: string; message: string } | null;
  intents: TestRunIntent[];
  logs: string[];
  fuel_used: number | null;
  ctx: Record<string, unknown>;
  module_sha256: string;
  simulated: true;
}
