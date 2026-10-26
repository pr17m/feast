import { manifestResponse } from "@/lib/pwa-manifest";

/** Restaurant app manifest ("Feast Partner"), linked from the /partner pages. */
export function GET() {
  return manifestResponse("partner");
}
