import type { Graph, TestRun } from "./api";

export const CODE_BLOCK_TYPE = "custom.code";
export const MAX_SOURCE_BYTES = 64 * 1024;

export const STARTER_SOURCE = `// decide(ctx) returns a list of intents. Limits are enforced by the server, not by this code:
// an over-limit stake is denied, a stake above your approval threshold waits for you.
function decide(ctx) {
  const move = ctx.market.move_bps;
  console.log("move", move);
  if (Math.abs(move) < 200) return [];
  return [{ type: "propose_challenge", stake: "2000000" }]; // 2.00 USDC, as a string of micro-USDC
}
`;

/** UTF-8 size of the source, the unit the server limits. */
export function sourceBytes(s: string): number {
  return new TextEncoder().encode(s).length;
}

export function byteCounterText(bytes: number): string {
  const kib = (bytes / 1024).toFixed(1);
  return `${bytes.toLocaleString("en-US")} / ${MAX_SOURCE_BYTES.toLocaleString("en-US")} bytes (${kib} KiB)`;
}

export function sourceProblem(s: string): string | null {
  if (s.trim() === "") return "Source is empty";
  if (sourceBytes(s) > MAX_SOURCE_BYTES) return "Source is over 64 KiB";
  return null;
}

export function usesCustomCode(graph: Graph): boolean {
  return graph.nodes.some((n) => n.type === CODE_BLOCK_TYPE);
}

const VERDICT: Record<string, string> = {
  allow: "Allowed",
  require_approval: "Needs your approval",
  deny: "Denied",
};

export function verdictLabel(result: string): string {
  return VERDICT[result] ?? result;
}

const REASON: Record<string, string> = {
  exceeds_max_per_interaction: "stake is above the max per interaction",
  exceeds_aggregate_exposure: "would exceed the max aggregate exposure",
  insufficient_available_balance: "not enough available balance",
  agent_paused: "agent is paused",
  stop_loss_triggered: "stop loss reached",
  interaction_type_not_allowed: "interaction type not allowed",
  zero_amount: "stake is zero",
  amount_overflow: "stake is too large",
  illegal_interaction_state: "not valid in this state",
};

export function reasonLabel(code: string): string {
  return REASON[code] ?? code.replace(/_/g, " ");
}

export function verdictTone(result: string): "pos" | "neg" | "pend" | "dim" {
  return result === "allow" ? "pos" : result === "deny" ? "neg" : result === "require_approval" ? "pend" : "dim";
}

/** One-line summary of a dry run for the header of the result. */
export function testRunSummary(r: TestRun): string {
  if (!r.ok) return `Failed: ${r.error?.code ?? "error"}`;
  const n = r.intents.length;
  return n === 0 ? "Ran fine and returned no intents" : `Returned ${n} intent${n === 1 ? "" : "s"}`;
}

/** Pull the failure fields out of a `custom_code_failed` activity `detail`. */
export function customCodeFailure(detail: Record<string, unknown> | null): { error: string; logs: string[]; fuel: number | null } | null {
  const code = detail?.code as { error?: unknown; logs?: unknown; fuel_used?: unknown } | undefined;
  if (!code || typeof code !== "object") return null;
  return {
    error: typeof code.error === "string" ? code.error : code.error ? JSON.stringify(code.error) : "unknown error",
    logs: Array.isArray(code.logs) ? code.logs.map(String) : [],
    fuel: typeof code.fuel_used === "number" ? code.fuel_used : null,
  };
}
