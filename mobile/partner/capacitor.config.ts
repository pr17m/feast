/// <reference types="@capacitor-firebase/messaging" />
/// <reference types="@capacitor/splash-screen" />
/// <reference types="@capacitor/status-bar" />
// Capacitor config for the Feast partner app. App id/name come from
// ../shared/apps.json and the allowed sites from ../shared/countries.json,
// so both apps stay in step. Run `npm run build` after editing those files.
import type { CapacitorConfig } from "@capacitor/cli";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// The Capacitor CLI runs from this folder (npm scripts cd here first).
const shared = join(process.cwd(), "..", "shared");
const app = JSON.parse(readFileSync(join(shared, "apps.json"), "utf8")).partner as { appId: string; appName: string; scheme: string };
const sites = JSON.parse(readFileSync(join(shared, "countries.json"), "utf8")) as {
  countries: { code: string; url: string }[];
  extraNavigationHosts: string[];
};

const config: CapacitorConfig = {
  appId: app.appId,
  appName: app.appName,
  webDir: "www",
  // Lets the server and analytics tell the apps apart from a mobile browser.
  appendUserAgent: "FeastApp/partner",
  backgroundColor: "#FAF7F2",
  server: {
    androidScheme: "https",
    hostname: "localhost",
    // The live country sites (and payment pages) open inside the app; any
    // other link opens in the system browser.
    allowNavigation: [...sites.countries.map((c) => new URL(c.url).host), ...sites.extraNavigationHosts],
    // Shown when the live site can't be reached (bundled, works offline).
    errorPath: "offline.html",
  },
  ios: {
    // Keep the live site clear of the notch / status bar.
    contentInset: "automatic",
  },
  android: {
    // Feast sites are https only.
    allowMixedContent: false,
  },
  plugins: {
    SplashScreen: {
      launchShowDuration: 600,
      launchAutoHide: true,
      backgroundColor: "#0F3D3A",
      showSpinner: false,
    },
    StatusBar: {
      style: "DARK",
      backgroundColor: "#0F3D3A",
      overlaysWebView: false,
    },
    FirebaseMessaging: {
      presentationOptions: ["alert", "badge", "sound"],
    },
  },
};

export default config;
