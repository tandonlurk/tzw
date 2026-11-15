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
