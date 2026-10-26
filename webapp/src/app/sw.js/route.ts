import { readFileSync } from "node:fs";
import path from "node:path";
import { SERVICE_WORKER_SOURCE } from "@/lib/service-worker";

/**
 * The service worker, served from the site root so it may control every page.
 * Its cache names carry the build id, so each deploy gets fresh caches and the
 * old ones are deleted when the new worker takes over.
 */
export const dynamic = "force-dynamic";

let buildId: string | undefined;
function currentBuildId(): string {
  if (buildId) return buildId;
  try {
    buildId = readFileSync(path.join(process.cwd(), ".next", "BUILD_ID"), "utf8").trim() || "dev";
  } catch {
    buildId = "dev";
  }
  return buildId;
}

export function GET() {
  return new Response(SERVICE_WORKER_SOURCE.replace("__FEAST_BUILD__", JSON.stringify(currentBuildId())), {
    headers: {
      "content-type": "application/javascript; charset=utf-8",
      // Browsers check for a new worker on navigation; never let a cache answer for them.
      "cache-control": "no-cache, no-store, must-revalidate",
      "service-worker-allowed": "/",
    },
  });
}
