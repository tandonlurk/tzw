// Basic / Advanced presentation modes. Pure: no React, no storage.
//
// Advanced routes live under /advanced. Pages without an advanced counterpart (wallet, challenges,
// firms, approvals, co-captain, settings, agent creation) are shared: they render in whichever mode
// the owner last used. Switching modes maps the current path to its counterpart so context is kept.

export type Mode = "basic" | "advanced";

export const MODE_COOKIE = "tzw-mode";
export const ADV = "/advanced";

/** The mode a path forces, or null for a shared page. */
export function modeOfPath(path: string): Mode | null {
  if (path === ADV || path.startsWith(`${ADV}/`)) return "advanced";
  if (path === "/" || path === "/agents" || path === "/activity") return "basic";
  const m = /^\/agents\/([^/]+)(\/.*)?$/.exec(path);
  if (m && m[1] !== "new") return "basic";
  return null;
}

const BASIC_TO_ADV_TAB: Record<string, string> = { "": "", "/logic": "/logic", "/activity": "/activity", "/edit": "/settings" };
const ADV_TO_BASIC_TAB: Record<string, string> = {
  "": "",
  "/performance": "",
  "/logic": "/logic",
  "/code": "/logic",
  "/activity": "/activity",
  "/settings": "/edit",
};

/** Path to open when switching to `target` from `path`. Shared pages keep their path. */
export function counterpartPath(path: string, target: Mode): string {
  const clean = path.length > 1 ? path.replace(/\/+$/, "") : path;
  if (target === "advanced") {
    if (modeOfPath(clean) === "advanced") return clean;
    if (clean === "/") return ADV;
    if (clean === "/agents") return `${ADV}/agents`;
    if (clean === "/activity") return `${ADV}/activity`;
    const m = /^\/agents\/([^/]+)(\/[^/]+)?$/.exec(clean);
    if (m && m[1] !== "new") return `${ADV}/agents/${m[1]}${BASIC_TO_ADV_TAB[m[2] ?? ""] ?? ""}`;
    return clean;
  }
  if (modeOfPath(clean) === "basic") return clean;
  if (clean === ADV) return "/";
  if (clean === `${ADV}/agents`) return "/agents";
  if (clean === `${ADV}/activity`) return "/activity";
  if (clean === `${ADV}/logic` || clean === `${ADV}/code`) return "/agents";
  const m = /^\/advanced\/agents\/([^/]+)(\/[^/]+)?$/.exec(clean);
  if (m) return `/agents/${m[1]}${ADV_TO_BASIC_TAB[m[2] ?? ""] ?? ""}`;
  if (clean.startsWith(`${ADV}/`)) return "/";
  return clean;
}

/** Agent id in an agent workspace path (either mode), else null. */
export function agentIdOfPath(path: string): string | null {
  const m = /^(?:\/advanced)?\/agents\/([^/]+)/.exec(path);
  return m && m[1] !== "new" ? decodeURIComponent(m[1]) : null;
}

export function parseMode(v: string | null | undefined): Mode {
  return v === "advanced" ? "advanced" : "basic";
}
