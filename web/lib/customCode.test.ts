import { describe, expect, it } from "vitest";
import type { Graph, TestRun } from "./api";
import { MAX_SOURCE_BYTES, byteCounterText, customCodeFailure, reasonLabel, sourceBytes, sourceProblem, STARTER_SOURCE, testRunSummary, usesCustomCode, verdictLabel, verdictTone } from "./customCode";

const run = (p: Partial<TestRun>): TestRun => ({ ok: true, error: null, intents: [], logs: [], fuel_used: 1, ctx: {}, module_sha256: "x", simulated: true, ...p });

describe("customCode helpers", () => {
  it("counts UTF-8 bytes, not characters", () => {
    expect(sourceBytes("abc")).toBe(3);
    expect(sourceBytes("é")).toBe(2);
    expect(sourceBytes("€")).toBe(3);
  });
  it("flags empty and oversized source", () => {
    expect(sourceProblem("  ")).toMatch(/empty/);
    expect(sourceProblem("a".repeat(MAX_SOURCE_BYTES))).toBeNull();
    expect(sourceProblem("a".repeat(MAX_SOURCE_BYTES + 1))).toMatch(/64 KiB/);
    expect(sourceProblem("€".repeat(Math.ceil(MAX_SOURCE_BYTES / 3) + 1))).toMatch(/64 KiB/);
  });
  it("starter defines decide and fits", () => {
    expect(STARTER_SOURCE).toContain("function decide(ctx)");
    expect(sourceProblem(STARTER_SOURCE)).toBeNull();
    expect(byteCounterText(1536)).toBe("1,536 / 65,536 bytes (1.5 KiB)");
  });
  it("detects custom code in a graph", () => {
    const g = (types: string[]): Graph => ({ schema_version: 1, nodes: types.map((t, i) => ({ id: `n${i}`, type: t, params: {} })), edges: [] });
    expect(usesCustomCode(g(["trigger.schedule", "custom.code"]))).toBe(true);
    expect(usesCustomCode(g(["trigger.schedule"]))).toBe(false);
  });
  it("labels verdicts and reasons", () => {
    expect(verdictLabel("require_approval")).toBe("Needs your approval");
    expect(verdictTone("deny")).toBe("neg");
    expect(reasonLabel("exceeds_max_per_interaction")).toContain("max per interaction");
    expect(reasonLabel("some_new_reason")).toBe("some new reason");
  });
  it("summarizes test runs", () => {
    expect(testRunSummary(run({}))).toMatch(/no intents/);
    expect(testRunSummary(run({ intents: [{ intent: { type: "reject" }, decision: { result: "allow", reasons: [] } }] }))).toBe("Returned 1 intent");
    expect(testRunSummary(run({ ok: false, error: { code: "out_of_fuel", message: "m" } }))).toBe("Failed: out_of_fuel");
  });
  it("reads failure detail", () => {
    expect(customCodeFailure({ code: { error: "boom", logs: ["a"], fuel_used: 5 } })).toEqual({ error: "boom", logs: ["a"], fuel: 5 });
    expect(customCodeFailure({})).toBeNull();
    expect(customCodeFailure(null)).toBeNull();
  });
});
