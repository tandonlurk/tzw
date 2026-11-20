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
  /** Firm payouts and task fees received; separate from challenge P&L. Absent on older backends. */
  firm_income?: Money;
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

export type BlockKind = "trigger" | "reasoning" | "condition" | "action" | "approach";

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
  /** Set when a firm's lead proposed the challenge (docs/design/approach-and-firms.md 3). Absent on older backends. */
  firm_id?: string | null;
  /** Arc testnet only: the escrow transactions so far (docs/api.md). Null on simulated challenges. */
  escrow?: EscrowTxs | null;
  resolution: "backend_administered";
  start_at: string;
  end_at: string;
  created_at: string;
  updated_at: string;
}

export interface EscrowTxs {
  create_tx: string | null;
  proposer_stake_tx: string | null;
  counterparty_stake_tx: string | null;
  settle_tx: string | null;
  /** Stays null when that vault had nothing staked. */
  proposer_release_tx: string | null;
  counterparty_release_tx: string | null;
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
  /** Escrow events delivered by QuickNode Streams (D-74); older backends omit it. */
  stream_events?: StreamEvent[];
}

/** How a QuickNode Streams event compares with the server's own confirmed transactions. */
export type StreamStatus = "pending" | "confirmed" | "mismatch" | "observed" | "unmatched";

export interface StreamEvent {
  id: string;
  /** Solidity event name: Created, Deposited, Activated, Resolved, Voided, Refunded, ... */
  event: string;
  status: StreamStatus;
  note: string | null;
  tx_hash: string;
  log_index: number;
  block_number: number;
  block_timestamp: string | null;
  received_at: string;
  /** received_at - block_timestamp; block timestamps are whole seconds. */
  latency_ms: number | null;
  removed: boolean;
  explorer_url: string | null;
}

/** GET /streams */
export interface StreamsStatus {
  webhook_enabled: boolean;
  matching_enabled: boolean;
  events_total: number;
  last_delivery_at: string | null;
  last_block_number: number | null;
  median_latency_ms: number | null;
  confirmed: number;
  pending: number;
  mismatch: number;
  observed: number;
  unmatched: number;
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
  /** Arc challenges that went active this tick. Absent on older backends. */
  activated?: number;
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

// ---- Arc testnet chain + agent vaults (D-57; contract in reports/handoffs/2026-11-17-wallet-ui.md) ----

/** `GET /chain`. When `enabled` is false the other fields are absent and the wallet UI stays off. */
export type ChainConfig =
  | { enabled: false }
  | {
      enabled: true;
      chain_id: number;
      chain_name: string;
      rpc_url: string;
      explorer_url: string;
      usdc_address: string;
      usdc_decimals: number;
      escrow_address: string;
      vault_factory_address: string;
      operator_address: string;
    };

export type EnabledChain = Extract<ChainConfig, { enabled: true }>;

/** `GET /agents/{id}/vault`: onchain state read by the backend, next to the agent's ledger balances. */
export interface VaultView {
  address: string;
  owner: string;
  operator: string;
  paused: boolean;
  max_per_challenge: Money;
  max_exposure: Money;
  onchain_balance: Money;
  onchain_exposure: Money;
  ledger_available: Money;
  ledger_committed: Money;
  /** Signed. Onchain minus ledger as computed by the backend; zero when they agree. */
  drift: Money;
  explorer_url: string;
}

export interface VaultSyncResult {
  credited: LedgerEntry[];
  vault: VaultView;
}

// ---- Approach block + firms (docs/design/approach-and-firms.md sections 1 to 3) ----

export type FirmRole = "none" | "member" | "lead";
export type MarketScope = "any" | "structure" | "specific";

/** Inclusive integer bounds; null = unbounded. */
export interface Bound {
  min: number | null;
  max: number | null;
}

export interface ApproachRanges {
  move_bps: Bound;
  yes_price_bps: Bound;
  expiry_secs: Bound;
  volume_delta_24h_bps: Bound;
}

/** The `markets` object of an `approach.profile` node; also a firm's `markets`. */
export interface ApproachMarkets {
  scope: MarketScope;
  structures: string[];
  categories: string[];
  subcategories: string[];
  keywords: string[];
  symbols: string[];
  ranges: ApproachRanges;
}

/** Params of the `approach.profile` graph node. */
export interface ApproachParams {
  stance: { collaboration_bps: number; ambition_bps: number };
  firm_role: FirmRole;
  markets: ApproachMarkets;
}

export interface MarketCategories {
  categories: string[];
  subcategories: Record<string, string[]>;
  structures: string[];
  source: "gemini" | "fallback";
}

/** A market as discovery sees it (`MarketView`). Only `title` is relied on; other fields are shown when present. */
export interface MarketSample {
  title: string;
  category?: string;
  subcategory?: string | null;
  structure?: string | null;
  move_bps?: number | null;
  yes_price_bps?: number | null;
  expiry_secs?: number | null;
  spot_symbol?: string | null;
  source?: string;
  [key: string]: unknown;
}

export interface MarketPreview {
  count: number;
  sample: MarketSample[];
  as_of: string;
  source: string;
}

export type FirmRoleName = "scout" | "analyst";
export type FirmState = "open" | "closed";

export interface FirmRoleSlot {
  role: FirmRoleName;
  slots: number;
  weight_bps: number;
}

export interface FirmMember {
  agent_id: string;
  name: string;
  role: FirmRoleName;
  joined_at: string;
  contributions: number;
  used: number;
  earned: Money;
}

export interface Firm {
  id: string;
  env: Env;
  name: string;
  mandate: string;
  lead_agent_id: string;
  lead_name: string;
  lead_share_bps: number;
  member_pool_bps: number;
  task_fee: Money;
  roles: FirmRoleSlot[];
  /** null = the lead's own approach markets. */
  markets: ApproachMarkets | null;
  state: FirmState;
  members: FirmMember[];
  stats: { challenges: number; settled: number; lead_realized_pnl: Money; paid_to_members: Money };
  /** Server-decided ("join", "leave", "close"); buttons are driven by this list only. */
  actions: string[];
  created_at: string;
}

export interface FirmInput {
  lead_agent_id: string;
  name: string;
  mandate: string;
  lead_share_bps?: number;
  task_fee?: Money;
  roles?: FirmRoleSlot[];
  markets?: ApproachMarkets | null;
}

export interface Contribution {
  id: string;
  firm_id: string;
  agent_id: string;
  agent_name: string;
  role: FirmRoleName;
  kind: "scout_candidate" | "analyst_signal";
  market_id: string;
  side: "above" | "below" | null;
  conviction_bps: number;
  interaction_id: string | null;
  used: boolean;
  fee_paid: Money;
  payout: Money;
  summary: string;
  created_at: string;
}
