import { describe, expect, it } from "vitest";
import type { ActivityItem, Firm } from "./api";
import {
  activityAmount,
  activityFirmId,
  activityKindLabel,
  activityKindTone,
  defaultRoles,
  memberPoolBps,
  openRoles,
  roleShareBps,
  splitProfit,
  validateRoles,
} from "./firms";

describe("firm profit split", () => {
  it("80/20 with one used contribution per role", () => {
    const r = splitProfit("10000000", 8000, [{ role: "scout", weight_bps: 5000 }, { role: "analyst", weight_bps: 5000 }]);
    expect(r).toEqual({ lead: "8000000", pool: "2000000", payouts: ["1000000", "1000000"] });
  });

  it("floors each payout and leaves the remainder with the lead", () => {
    // p = 1000001 → pool = 200000 (floor of 200000.2); three scouts split 200000/3 = 66666 each
    const r = splitProfit("1000001", 8000, [
      { role: "scout", weight_bps: 6000 },
      { role: "scout", weight_bps: 6000 },
      { role: "scout", weight_bps: 6000 },
    ]);
    expect(r.pool).toBe("200000");
    expect(r.payouts).toEqual(["66666", "66666", "66666"]);
    expect(r.lead).toBe(String(1000001 - 3 * 66666));
    expect(BigInt(r.lead) + r.payouts.reduce((s, x) => s + BigInt(x), BigInt(0))).toBe(BigInt(1000001));
  });

  it("renormalises over roles that contributed; equal within a role", () => {
    const r = splitProfit("1000", 8000, [
      { role: "scout", weight_bps: 3000 },
      { role: "analyst", weight_bps: 7000 },
      { role: "analyst", weight_bps: 7000 },
    ]);
    // pool 200: scout 200*3000/10000 = 60; analysts 200*7000/(10000*2) = 70 each
    expect(r.payouts).toEqual(["60", "70", "70"]);
    expect(r.lead).toBe("800");
    // only scouts used: they get the whole pool
    expect(splitProfit("1000", 8000, [{ role: "scout", weight_bps: 3000 }]).payouts).toEqual(["200"]);
  });

  it("losses and zero profit are the lead's alone; 100% lead pays nothing", () => {
    expect(splitProfit("-500", 8000, [{ role: "scout", weight_bps: 5000 }])).toEqual({ lead: "-500", pool: "0", payouts: ["0"] });
    expect(splitProfit("0", 8000, [])).toEqual({ lead: "0", pool: "0", payouts: [] });
    expect(splitProfit("1000", 10000, [{ role: "scout", weight_bps: 10000 }])).toEqual({ lead: "1000", pool: "0", payouts: ["0"] });
    expect(splitProfit("1000", 8000, []).lead).toBe("1000");
  });

  it("role shares for the split bar", () => {
    expect(memberPoolBps(8000)).toBe(2000);
    expect(roleShareBps(8000, defaultRoles())).toEqual([{ role: "scout", bps: 1000 }, { role: "analyst", bps: 1000 }]);
    expect(roleShareBps(7500, [{ role: "scout", slots: 1, weight_bps: 3333 }])).toEqual([{ role: "scout", bps: 833 }]);
  });
});

describe("role validation", () => {
  it("accepts the defaults", () => {
    expect(validateRoles(defaultRoles())).toEqual([]);
    expect(validateRoles([{ role: "scout", slots: 5, weight_bps: 10000 }])).toEqual([]);
  });
  it("rejects bad weights, slots, duplicates and empty lists", () => {
    expect(validateRoles([])).toContain("Add at least one role.");
    expect(validateRoles([{ role: "scout", slots: 1, weight_bps: 6000 }, { role: "analyst", slots: 1, weight_bps: 3000 }]).join()).toMatch(/add up to 100% \(now 90%\)/);
    expect(validateRoles([{ role: "scout", slots: 0, weight_bps: 10000 }]).join()).toMatch(/slots must be 1 to 5/);
    expect(validateRoles([{ role: "scout", slots: 6, weight_bps: 10000 }]).join()).toMatch(/slots/);
    expect(validateRoles([{ role: "scout", slots: 1, weight_bps: 5000 }, { role: "scout", slots: 1, weight_bps: 5000 }]).join()).toMatch(/twice/);
  });
});

describe("firm helpers", () => {
  const firm = {
    roles: [{ role: "scout", slots: 1, weight_bps: 5000 }, { role: "analyst", slots: 2, weight_bps: 5000 }],
    members: [{ agent_id: "a", name: "A", role: "scout", joined_at: "", contributions: 0, used: 0, earned: "0" }],
  } as unknown as Firm;
  it("open roles", () => expect(openRoles(firm)).toEqual(["analyst"]));

  it("activity kinds", () => {
    expect(activityKindLabel("firm_payout")).toBe("firm payout");
    expect(activityKindTone("firm_fee_denied")).toBe("neg");
    expect(activityKindLabel("challenge_settled")).toBe("challenge settled");
    const it0 = { id: "1", ts: "", agent_id: null, kind: "firm_task_fee", summary: "", detail: { firm_id: "f1", amount: "250000" } } as ActivityItem;
    expect(activityFirmId(it0)).toBe("f1");
    expect(activityAmount(it0)).toBe("250000");
    expect(activityFirmId({ ...it0, detail: null })).toBeNull();
  });
});
