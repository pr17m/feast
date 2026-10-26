import { androidApps, assetLinks } from "@/lib/app-links";

export const dynamic = "force-dynamic";

/** Android App Links: GET /.well-known/assetlinks.json. */
export function GET() {
  const apps = androidApps();
  if (!apps.length) return new Response("Not configured (set FEAST_ANDROID_APPS)\n", { status: 404, headers: { "content-type": "text/plain" } });
  return new Response(JSON.stringify(assetLinks(apps)), {
    headers: { "content-type": "application/json", "cache-control": "public, max-age=3600" },
  });
}
