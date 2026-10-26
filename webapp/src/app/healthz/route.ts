/** Liveness probe for load balancers and container platforms. */
export const dynamic = "force-dynamic";

export function GET() {
  return Response.json({ status: "ok" }, { headers: { "cache-control": "no-store" } });
}
