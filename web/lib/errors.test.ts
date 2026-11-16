import { describe, expect, it } from "vitest";
import { unreachableHint } from "./errors";

describe("unreachableHint", () => {
  it("names the backend, the CORS origin and the page origin", () => {
    const h = unreachableHint("http://127.0.0.1:3013", "http://localhost:8094/api");
    expect(h).toContain("http://localhost:8094/api");
    expect(h).toContain("http://127.0.0.1:3013");
    expect(h).toContain("CORS_ORIGIN");
  });
  it("works without a page origin (server render)", () => {
    expect(unreachableHint(null, "x")).not.toContain("opened from");
  });
});
