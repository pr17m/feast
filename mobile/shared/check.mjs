// CI check (no Xcode / Android SDK needed): the shared settings are valid and
// the committed native projects match what shared/configure-native.mjs makes.
import { execSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const shared = dirname(fileURLToPath(import.meta.url));
const root = dirname(shared);
const fail = (m) => {
  console.error(`✗ ${m}`);
  process.exitCode = 1;
};

const apps = JSON.parse(readFileSync(join(shared, "apps.json"), "utf8"));
const { countries } = JSON.parse(readFileSync(join(shared, "countries.json"), "utf8"));
for (const c of countries) if (!/^https:\/\/[a-z0-9.-]+$/.test(c.url.replace(/\/+$/, ""))) fail(`countries.json: ${c.code} must be https://host`);
for (const [k, a] of Object.entries(apps)) {
  if (k.startsWith("_")) continue;
  if (!/^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)+$/.test(a.appId)) fail(`apps.json: ${k}.appId is not a valid bundle id`);
  if (!/^[a-z][a-z0-9+.-]*$/.test(a.scheme)) fail(`apps.json: ${k}.scheme is not a valid URL scheme`);
  for (const plat of ["android", "ios"]) {
    const cfg = JSON.parse(
      readFileSync(join(root, k, plat === "android" ? "android/app/src/main/assets/capacitor.config.json" : "ios/App/App/capacitor.config.json"), "utf8"),
    );
    if (cfg.appId !== a.appId) fail(`${k}/${plat}: appId ${cfg.appId} != ${a.appId}`);
    for (const c of countries) if (!cfg.server.allowNavigation.includes(new URL(c.url).host)) fail(`${k}/${plat}: ${c.url} missing from allowNavigation`);
  }
}
const paths = "-- diner/android diner/ios partner/android partner/ios";
const diff = [execSync(`git diff --name-only ${paths}`, { cwd: root }), execSync(`git ls-files --others --exclude-standard ${paths}`, { cwd: root })]
  .map(String)
  .join("")
  .trim();
if (diff) fail(`native projects differ from shared/configure-native.mjs output — run \`npm run build\` and commit:\n${diff}`);
if (!process.exitCode) console.log("✓ mobile config OK");
