/**
 * Deploy-time configuration for the browser. The same web build serves every
 * country: each deployment sets its own env vars, read here at request time.
 *
 *   FEAST_API_URL   this country's API, e.g. https://api.au.feast.example,
 *                   or "/" when the API is served on the same domain (/v1/…)
 *   FEAST_COUNTRY   e.g. "au" (informational; the API decides the market)
 *   FEAST_API_INTERNAL_URL  private API URL for the built-in /v1 proxy (app/v1/[...path])
 */
export const dynamic = "force-dynamic";

export function GET() {
  const config = {
    // With the built-in proxy configured, the browser calls the API on this same origin.
    apiBaseUrl: process.env.FEAST_API_URL ?? (process.env.FEAST_API_INTERNAL_URL ? "/" : undefined) ?? process.env.NEXT_PUBLIC_API_BASE_URL ?? "http://localhost:8080",
    country: (process.env.FEAST_COUNTRY ?? "").toUpperCase(),
  };
  return new Response(`window.__FEAST_CONFIG__=${JSON.stringify(config)};`, {
    headers: { "content-type": "application/javascript; charset=utf-8", "cache-control": "no-store" },
  });
}
