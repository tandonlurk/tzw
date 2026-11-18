import type { NextRequest } from "next/server";
import { checkRequest, parseAllowedHosts, targetUrl, upstreamHeaders } from "@/lib/proxy";

// Same-origin proxy to the backend that adds AEN_API_TOKEN server-side (D-70). Used only when
// NEXT_PUBLIC_API_URL=/backend; the default setup calls the backend directly and never hits this.
// Server-only env: AEN_API_URL (backend base, default http://localhost:8080/api), AEN_API_TOKEN,
// WEB_ALLOWED_HOSTS (extra Host names besides loopback).

export const dynamic = "force-dynamic";

const API_URL = process.env.AEN_API_URL ?? "http://localhost:8080/api";

function error(status: number, code: string, message: string): Response {
  return Response.json({ error: { code, message, details: [] } }, { status });
}

async function proxy(req: NextRequest): Promise<Response> {
  const check = checkRequest(req.headers.get("host"), req.headers.get("origin"), parseAllowedHosts(process.env.WEB_ALLOWED_HOSTS));
  if (!check.ok) return error(check.status, check.code, check.message);

  const url = targetUrl(API_URL, req.nextUrl.pathname, req.nextUrl.search);
  if (!url) return error(400, "bad_path", "Invalid proxy path");

  const hasBody = req.method !== "GET" && req.method !== "HEAD";
  let upstream: Response;
  try {
    upstream = await fetch(url, {
      method: req.method,
      headers: upstreamHeaders(req.headers, process.env.AEN_API_TOKEN || undefined),
      body: hasBody ? await req.arrayBuffer() : undefined,
      cache: "no-store",
      redirect: "manual",
    });
  } catch {
    return error(502, "upstream_unreachable", "The web server cannot reach the backend");
  }

  const headers = new Headers();
  const ct = upstream.headers.get("content-type");
  if (ct) headers.set("content-type", ct);
  return new Response(upstream.body, { status: upstream.status, headers });
}

export const GET = proxy;
export const POST = proxy;
export const PUT = proxy;
export const DELETE = proxy;
export const PATCH = proxy;
