import { describe, expect, it } from "vitest";
import { counterpartPath } from "./mode";
import { safeEvidenceUrl, sparringPrompt } from "./sparring";

describe("sparring sources", () => {
  it("links only web sources without embedded credentials", () => {
    expect(safeEvidenceUrl("https://example.com/report?q=price")).toBe("https://example.com/report?q=price");
    for (const source of ["javascript:alert(1)", "data:text/html,bad", "/relative", "https://key:secret@example.com", "not a url"]) expect(safeEvidenceUrl(source)).toBeNull();
  });
});

describe("sparring handoff", () => {
  it("targets the restricted bot endpoints and keeps secrets out of the prompt", () => {
    const prompt = sparringPrompt("test-session");
    expect(prompt).toContain("GET /api/bot/sparring/test-session");
    expect(prompt).toContain("POST your critique to /api/bot/sparring/test-session/rounds");
    expect(prompt).toContain("untrusted material");
    expect(prompt).toContain("do not approve transactions");
    expect(prompt).not.toContain("Bearer");
  });
  it("preserves the practice page when switching modes", () => {
    expect(counterpartPath("/agents/a/sparring", "advanced")).toBe("/advanced/agents/a/sparring");
    expect(counterpartPath("/advanced/agents/a/sparring", "basic")).toBe("/agents/a/sparring");
  });
});
