/**
 * Same-origin backend proxy (D-70). Pure helpers; the route handler is `app/backend/[...path]/route.ts`.
 *
 * When the backend runs with `AEN_API_TOKEN` (SEC-02), the browser must not hold the token: anything
 * `NEXT_PUBLIC_*` ends up in the JS bundle. Instead the browser calls `/backend/...` on the web server,
 * which adds `Authorization: Bearer <AEN_API_TOKEN>` from its own (server-only) env and forwards to
 * `AEN_API_URL`. The proxy keeps the backend's two browser-facing checks for itself: Host must be
 * loopback or listed in `WEB_ALLOWED_HOSTS` (DNS rebinding), and a present Origin must match Host (CSRF).
 */

export const PROXY_PREFIX = "/backend";

const LOOPBACK = new Set(["localhost", "127.0.0.1", "[::1]", "::1"]);

/** Host header without its port, lowercased. IPv6 literals keep their brackets. */
export function hostName(host: string): string {
  const h = host.trim().toLowerCase();
  if (h.startsWith("[")) {
    const end = h.indexOf("]");
    return end === -1 ? h : h.slice(0, end + 1);
  }
  const colon = h.indexOf(":");
  return colon === -1 ? h : h.slice(0, colon);
}

export function parseAllowedHosts(raw: string | undefined): string[] {
  return (raw ?? "")
    .split(",")
    .map((s) => hostName(s))
    .filter((s) => s.length > 0);
}

export type ProxyCheck = { ok: true } | { ok: false; status: number; code: string; message: string };

/** Host allowlist + Origin check, mirroring the backend guard (SEC-02). */
export function checkRequest(host: string | null, origin: string | null, allowedHosts: string[]): ProxyCheck {
  if (!host) return { ok: false, status: 400, code: "host_missing", message: "Host header is required" };
  const name = hostName(host);
  if (!LOOPBACK.has(name) && !allowedHosts.includes(name)) {
    return { ok: false, status: 403, code: "host_not_allowed", message: `Host ${name} is not allowed` };
  }
  if (origin !== null) {
    let originHost: string;
    try {
      originHost = new URL(origin).host.toLowerCase();
    } catch {
      return { ok: false, status: 403, code: "origin_not_allowed", message: "Origin is not allowed" };
    }
    if (originHost !== host.trim().toLowerCase()) {
      return { ok: false, status: 403, code: "origin_not_allowed", message: "Origin is not allowed" };
    }
  }
  return { ok: true };
}

/**
 * Backend URL for a proxied request. `pathname` must start with `/backend/`; dot segments are refused
 * so the proxy cannot be pointed outside the API base.
 */
export function targetUrl(apiUrl: string, pathname: string, search: string): string | null {
  if (!pathname.startsWith(`${PROXY_PREFIX}/`)) return null;
  const rest = pathname.slice(PROXY_PREFIX.length);
  if (rest.split("/").some((seg) => seg === ".." || seg === "." || /%2e/i.test(seg) || /%2f/i.test(seg))) return null;
  return `${apiUrl.replace(/\/+$/, "")}${rest}${search}`;
}

const FORWARDED = ["content-type", "accept", "idempotency-key"];

/** Headers sent upstream: content type, accept, idempotency key, and the bearer token. Cookies and the caller's own Authorization are dropped. */
export function upstreamHeaders(incoming: Headers, token: string | undefined): Headers {
  const out = new Headers();
  for (const name of FORWARDED) {
    const v = incoming.get(name);
    if (v) out.set(name, v);
  }
  if (token) out.set("authorization", `Bearer ${token}`);
  return out;
}
