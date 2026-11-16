import { describe, expect, it } from "vitest";
import { ApiError } from "./api";
import { approvalRequiredParties, collabActionLabel, collabStateTone, formatBps, isCollabOpen, parseSharePercent } from "./collaborations";

describe("collaborations helpers", () => {
  it("formats and parses shares", () => {
    expect(formatBps(6000)).toBe("60%");
    expect(formatBps(3333)).toBe("33.33%");
    expect(formatBps(5050)).toBe("50.5%");
    expect(parseSharePercent("60")).toBe(6000);
    expect(parseSharePercent("33.33")).toBe(3333);
    expect(parseSharePercent("100")).toBe(10000);
    expect(parseSharePercent("100.01")).toBeNull();
    expect(parseSharePercent("abc")).toBeNull();
    expect(parseSharePercent("1.234")).toBeNull();
    expect(parseSharePercent("")).toBeNull();
  });

  it("tones, openness and labels", () => {
    expect(collabStateTone("completed")).toBe("pos");
    expect(collabStateTone("cancelled")).toBe("neg");
    expect(collabStateTone("funded")).toBe("info");
    expect(isCollabOpen("funded")).toBe(true);
    expect(isCollabOpen("completed")).toBe(false);
    expect(collabActionLabel("complete")).toContain("complete");
  });

  it("reads approval_required parties", () => {
    const e = new ApiError("approval_required", "x", 409, [{ path: "partner", code: "approval_required", message: "m" }, { path: "partner" }]);
    expect(approvalRequiredParties(e)).toEqual(["partner"]);
    expect(approvalRequiredParties(new ApiError("invalid_state", "x", 409))).toBeNull();
    expect(approvalRequiredParties(null)).toBeNull();
  });
});
