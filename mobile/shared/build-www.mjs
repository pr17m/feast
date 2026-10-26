// Builds each app's bundled start page (www/) from shared/www and the
// settings in shared/apps.json + shared/countries.json.
// Usage: node shared/build-www.mjs [diner|partner ...]   (default: both)
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const shared = dirname(fileURLToPath(import.meta.url));
const root = dirname(shared);
const apps = JSON.parse(readFileSync(join(shared, "apps.json"), "utf8"));
const { countries } = JSON.parse(readFileSync(join(shared, "countries.json"), "utf8"));

for (const c of countries) {
  if (!/^https:\/\/[^/]+$/.test(c.url.replace(/\/+$/, ""))) throw new Error(`countries.json: ${c.code} url must be https://host (got ${c.url})`);
}

const wanted = process.argv.slice(2).length ? process.argv.slice(2) : ["diner", "partner"];
for (const name of wanted) {
  const app = apps[name];
  if (!app) throw new Error(`Unknown app ${name}`);
  const www = join(root, name, "www");
  rmSync(www, { recursive: true, force: true });
  mkdirSync(www, { recursive: true });
  cpSync(join(shared, "www"), www, { recursive: true });
  copyFileSync(join(shared, "assets", `${name}-icon.png`), join(www, "icon.png"));
  const cfg = { app: name, appName: app.appName, tagline: app.tagline, startPath: app.startPath, scheme: app.scheme, countries };
  writeFileSync(join(www, "config.js"), `window.FEAST_APP = ${JSON.stringify(cfg, null, 2)};\n`);
  const placeholders = countries.filter((c) => /\.example(\/|$)/.test(c.url)).map((c) => c.code);
  console.log(`${name}: www built${placeholders.length ? ` (placeholder URLs still set for: ${placeholders.join(", ")})` : ""}`);
}
