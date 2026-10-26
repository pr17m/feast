/**
 * Same-origin API gateway: forwards /v1/* to this country's private API
 * (FEAST_API_INTERNAL_URL, e.g. http://api:8080). The web app is then the only
 * public endpoint on every platform (Docker, ECS, Container Apps, Cloud Run,
 * Kubernetes), and the browser uses relative URLs (FEAST_API_URL="/").
 *
 * Streams both ways: document uploads in, Server-Sent Events (/v1/me/events) out.
 */
export const dynamic = "force-dynamic";
export const runtime = "nodejs";

const HOP_BY_HOP = new Set(["connection", "keep-alive", "transfer-encoding", "upgrade", "proxy-authenticate", "proxy-authorization", "te", "trailer", "host", "content-length"]);

async function forward(req: Request, ctx: { params: Promise<{ path: string[] }> }): Promise<Response> {
  const upstream = process.env.FEAST_API_INTERNAL_URL;
  if (!upstream) {
    return Response.json({ code: "NOT_CONFIGURED", message: "API proxy is not configured (set FEAST_API_INTERNAL_URL)" }, { status: 502 });
  }
  const { path } = await ctx.params;
  const incoming = new URL(req.url);
  const target = new URL(`/v1/${path.map(encodeURIComponent).join("/")}${incoming.search}`, upstream);

  const headers = new Headers();
  req.headers.forEach((v, k) => {
    if (!HOP_BY_HOP.has(k)) headers.set(k, v);
  });
  headers.set("x-forwarded-host", incoming.host);
  headers.set("x-forwarded-proto", incoming.protocol.replace(":", ""));

  const hasBody = req.method !== "GET" && req.method !== "HEAD";
  let res: Response;
  try {
    res = await fetch(target, {
      method: req.method,
      headers,
      body: hasBody ? req.body : undefined,
      redirect: "manual",
      cache: "no-store",
      signal: req.signal,
      // Required by Node to stream a request body.
      ...(hasBody ? { duplex: "half" } : {}),
    } as RequestInit);
  } catch {
    return Response.json({ code: "UNAVAILABLE", message: "Feast is temporarily unavailable. Please try again." }, { status: 503 });
  }
  const out = new Headers();
  res.headers.forEach((v, k) => {
    if (!HOP_BY_HOP.has(k) && k !== "content-encoding") out.set(k, v);
  });
  if (res.headers.get("content-type")?.startsWith("text/event-stream")) {
    out.set("cache-control", "no-cache, no-transform");
    out.set("x-accel-buffering", "no");
  }
  return new Response(res.body, { status: res.status, statusText: res.statusText, headers: out });
}

export { forward as GET, forward as POST, forward as PUT, forward as PATCH, forward as DELETE };
