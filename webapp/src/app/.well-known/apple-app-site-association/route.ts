import { appleAppSiteAssociation, iosAppIds } from "@/lib/app-links";

export const dynamic = "force-dynamic";

/** iOS universal links: GET /.well-known/apple-app-site-association (JSON, no redirect). */
export function GET() {
  const ids = iosAppIds();
  if (!ids.length) return new Response("Not configured (set FEAST_IOS_APP_IDS)\n", { status: 404, headers: { "content-type": "text/plain" } });
  return new Response(JSON.stringify(appleAppSiteAssociation(ids)), {
    headers: { "content-type": "application/json", "cache-control": "public, max-age=3600" },
  });
}
