import { manifestResponse } from "@/lib/pwa-manifest";

/** Diner app manifest ("Feast"), linked from every page outside /partner and /admin. */
export function GET() {
  return manifestResponse("diner");
}
