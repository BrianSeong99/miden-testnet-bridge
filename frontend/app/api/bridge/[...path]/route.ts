import { NextRequest } from "next/server";

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

const bridgeBase = process.env.BRIDGE_API_BASE ?? "http://bridge:8080";
const hopByHopHeaders = new Set([
  "connection",
  "content-encoding",
  "content-length",
  "host",
  "keep-alive",
  "proxy-authenticate",
  "proxy-authorization",
  "te",
  "trailer",
  "transfer-encoding",
  "upgrade",
]);

type RouteContext = {
  params: Promise<{ path: string[] }>;
};

function targetUrl(path: string[], request: NextRequest) {
  const requestUrl = new URL(request.url);
  const upstreamPath = `/${path.map(encodeURIComponent).join("/")}`;
  const target = new URL(upstreamPath, bridgeBase);
  target.search = requestUrl.search;
  return target;
}

function requestHeaders(request: NextRequest) {
  const headers = new Headers(request.headers);
  for (const header of hopByHopHeaders) {
    headers.delete(header);
  }
  return headers;
}

function responseHeaders(upstream: Response) {
  const headers = new Headers(upstream.headers);
  for (const header of hopByHopHeaders) {
    headers.delete(header);
  }
  return headers;
}

async function proxy(request: NextRequest, context: RouteContext) {
  const { path } = await context.params;
  const method = request.method.toUpperCase();
  const body = method === "GET" || method === "HEAD" ? undefined : await request.arrayBuffer();

  try {
    const upstream = await fetch(targetUrl(path, request), {
      method,
      headers: requestHeaders(request),
      body,
      cache: "no-store",
      redirect: "manual",
    });

    return new Response(upstream.body, {
      status: upstream.status,
      statusText: upstream.statusText,
      headers: responseHeaders(upstream),
    });
  } catch (error) {
    return Response.json(
      {
        message: "Bridge API is unavailable",
        detail: error instanceof Error ? error.message : String(error),
      },
      { status: 502 },
    );
  }
}

export const GET = proxy;
export const POST = proxy;
export const PUT = proxy;
export const PATCH = proxy;
export const DELETE = proxy;
export const HEAD = proxy;
