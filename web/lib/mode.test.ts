import { describe, expect, it } from "vitest";
import { agentIdOfPath, counterpartPath, modeOfPath } from "./mode";

describe("modeOfPath", () => {
  it("forces advanced under /advanced", () => {
    expect(modeOfPath("/advanced")).toBe("advanced");
    expect(modeOfPath("/advanced/agents/a1/logic")).toBe("advanced");
    expect(modeOfPath("/advancedx")).toBe(null);
  });
  it("forces basic on basic-only pages", () => {
    expect(modeOfPath("/")).toBe("basic");
    expect(modeOfPath("/agents")).toBe("basic");
    expect(modeOfPath("/agents/a1/edit")).toBe("basic");
  });
  it("leaves shared pages to the stored preference", () => {
    for (const p of ["/wallet", "/approvals", "/interactions/x", "/firms", "/settings", "/agents/new"]) expect(modeOfPath(p)).toBe(null);
  });
});

describe("counterpartPath", () => {
  it("maps basic pages to their advanced workspace and keeps the agent", () => {
    expect(counterpartPath("/", "advanced")).toBe("/advanced");
    expect(counterpartPath("/agents/a1", "advanced")).toBe("/advanced/agents/a1");
    expect(counterpartPath("/agents/a1/logic", "advanced")).toBe("/advanced/agents/a1/logic");
    expect(counterpartPath("/agents/a1/edit", "advanced")).toBe("/advanced/agents/a1/settings");
    expect(counterpartPath("/activity", "advanced")).toBe("/advanced/activity");
  });
  it("maps advanced pages back", () => {
    expect(counterpartPath("/advanced", "basic")).toBe("/");
    expect(counterpartPath("/advanced/agents/a1/performance", "basic")).toBe("/agents/a1");
    expect(counterpartPath("/advanced/agents/a1/code", "basic")).toBe("/agents/a1/logic");
    expect(counterpartPath("/advanced/agents/a1/settings", "basic")).toBe("/agents/a1/edit");
    expect(counterpartPath("/advanced/logic", "basic")).toBe("/agents");
  });
  it("keeps shared pages and is idempotent", () => {
    expect(counterpartPath("/wallet", "advanced")).toBe("/wallet");
    expect(counterpartPath("/agents/new", "advanced")).toBe("/agents/new");
    expect(counterpartPath("/advanced/agents", "advanced")).toBe("/advanced/agents");
    expect(counterpartPath("/agents/", "advanced")).toBe("/advanced/agents");
  });
});

describe("agentIdOfPath", () => {
  it("finds the agent in either mode", () => {
    expect(agentIdOfPath("/agents/a1/logic")).toBe("a1");
    expect(agentIdOfPath("/advanced/agents/a%201")).toBe("a 1");
    expect(agentIdOfPath("/agents/new")).toBe(null);
    expect(agentIdOfPath("/wallet")).toBe(null);
  });
});
