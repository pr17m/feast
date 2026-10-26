// Copies the shared icon/splash sources into each app's assets/ folder in the
// layout @capacitor/assets expects (icon-only, icon-foreground,
// icon-background, splash, splash-dark). Edit the SVGs in shared/assets,
// re-export the PNGs (1024px icons, 2732px splash), then run `npm run assets`.
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const shared = dirname(fileURLToPath(import.meta.url));
const root = dirname(shared);
const map = {
  "icon-only.png": "icon.png",
  "icon-foreground.png": "icon-foreground.png",
  "icon-background.png": "icon-background.png",
  "splash.png": "splash.png",
  "splash-dark.png": "splash-dark.png",
};
for (const app of ["diner", "partner"]) {
  const out = join(root, app, "assets");
  mkdirSync(out, { recursive: true });
  for (const [to, from] of Object.entries(map)) copyFileSync(join(shared, "assets", `${app}-${from}`), join(out, to));
  console.log(`${app}: assets copied`);
}
