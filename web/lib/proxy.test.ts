import { describe, expect, it } from "vitest";
import { checkRequest, hostName, parseAllowedHosts, targetUrl, upstreamHeaders } from "./proxy";

describe("hostName", () => {
  it("strips the port and lowercases", () => {
    expect(hostName("LocalHost:3000")).toBe("localhost");
    expect(hostName("127.0.0.1")).toBe("127.0.0.1");
    expect(hostName("[::1]:3000")).toBe("[::1]");
  });
});

describe("checkRequest", () => {
  it("accepts loopback hosts with and without a same-origin Origin", () => {
    expect(checkRequest("localhost:3000", null, [])).toEqual({ ok: true });
    expect(checkRequest("127.0.0.1:3000", "http://127.0.0.1:3000", [])).toEqual({ ok: true });
    expect(checkRequest("[::1]:3000", "http://[::1]:3000", [])).toEqual({ ok: true });
  });

  it("refuses a rebinding Host unless allowlisted", () => {
    const r = checkRequest("evil.example:3000", "http://evil.example:3000", []);
    expect(r).toMatchObject({ ok: false, status: 403, code: "host_not_allowed" });
    expect(checkRequest("demo.example:3000", null, parseAllowedHosts(" Demo.example , "))).toEqual({ ok: true });
  });

  it("refuses a cross-site Origin even on a loopback Host", () => {
    expect(checkRequest("localhost:3000", "http://evil.example", [])).toMatchObject({ ok: false, code: "origin_not_allowed" });
    expect(checkRequest("localhost:3000", "http://localhost:4000", [])).toMatchObject({ ok: false, code: "origin_not_allowed" });
    expect(checkRequest("localhost:3000", "null", [])).toMatchObject({ ok: false, code: "origin_not_allowed" });
  });

  it("refuses a missing Host", () => {
    expect(checkRequest(null, null, [])).toMatchObject({ ok: false, status: 400 });
  });
});

describe("targetUrl", () => {
  const api = "http://localhost:8080/api/";
  it("maps /backend/... onto the API base with the query", () => {
    expect(targetUrl(api, "/backend/agents", "")).toBe("http://localhost:8080/api/agents");
    expect(targetUrl(api, "/backend/stats/portfolio", "?env=arc_testnet")).toBe(
      "http://localhost:8080/api/stats/portfolio?env=arc_testnet",
    );
  });
  it("refuses other prefixes and dot segments", () => {
    expect(targetUrl(api, "/backendx/agents", "")).toBeNull();
    expect(targetUrl(api, "/backend/../health", "")).toBeNull();
    expect(targetUrl(api, "/backend/%2e%2e/x", "")).toBeNull();
    expect(targetUrl(api, "/backend/a%2Fb", "")).toBeNull();
  });
});

describe("upstreamHeaders", () => {
  it("adds the token and forwards only safe headers", () => {
    const h = upstreamHeaders(
      new Headers({
        "content-type": "application/json",
        "idempotency-key": "k1",
        cookie: "session=x",
        authorization: "Bearer attacker",
        origin: "http://localhost:3000",
      }),
      "t".repeat(32),
    );
    expect(h.get("authorization")).toBe(`Bearer ${"t".repeat(32)}`);
    expect(h.get("content-type")).toBe("application/json");
    expect(h.get("idempotency-key")).toBe("k1");
    expect(h.get("cookie")).toBeNull();
    expect(h.get("origin")).toBeNull();
  });
  it("sends no Authorization without a token, even if the caller sent one", () => {
    expect(upstreamHeaders(new Headers({ authorization: "Bearer x" }), undefined).get("authorization")).toBeNull();
  });
});
