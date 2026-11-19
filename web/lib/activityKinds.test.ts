import { describe, expect, it } from "vitest";
import { categoryOf, dayLabel, groupByDay } from "./activityKinds";

describe("categoryOf", () => {
  it("maps known kinds", () => {
    expect(categoryOf("challenge_proposed")).toBe("challenge");
    expect(categoryOf("challenge_countered")).toBe("negotiation");
    expect(categoryOf("challenge_settled")).toBe("settlement");
    expect(categoryOf("approval_requested")).toBe("approval");
    expect(categoryOf("stake_committed")).toBe("transaction");
    expect(categoryOf("firm_payout")).toBe("settlement");
    expect(categoryOf("firm_joined")).toBe("collaboration");
    expect(categoryOf("approach_filtered")).toBe("discovery");
  });
  it("falls back to agent for unknown kinds", () => {
    expect(categoryOf("something_new")).toBe("agent");
  });
});

describe("day grouping", () => {
  const now = new Date(2026, 10, 19, 12);
  it("labels today and yesterday", () => {
    expect(dayLabel(new Date(2026, 10, 19, 1).toISOString(), now)).toBe("Today");
    expect(dayLabel(new Date(2026, 10, 18, 23).toISOString(), now)).toBe("Yesterday");
  });
  it("groups consecutive items", () => {
    const g = groupByDay(
      [new Date(2026, 10, 19, 3), new Date(2026, 10, 19, 2), new Date(2026, 10, 18, 2)].map((d) => ({ ts: d.toISOString() })),
      now,
    );
    expect(g.map((x) => [x.label, x.items.length])).toEqual([["Today", 2], ["Yesterday", 1]]);
  });
});
