/**
 * Universal links (iOS) and App Links (Android): which store apps may open
 * Feast URLs. Read at request time from the deployment's env:
 *
 *   FEAST_IOS_APP_IDS    comma-separated "<TeamID>.<bundle id>", e.g.
 *                        ABCDE12345.com.feast.diner,ABCDE12345.com.feast.partner
 *   FEAST_ANDROID_APPS   comma-separated "<package>:<SHA-256 cert fingerprint>",
 *                        e.g. com.feast.diner:AB:CD:…:EF,com.feast.partner:12:34:…
 *                        (several fingerprints for one package: separate with |)
 *
 * Every path opens in the app except the API (/v1/*) and the admin app (/admin/*).
 */

export const EXCLUDED_PATHS = ["/v1/*", "/admin/*"] as const;

const list = (v: string | undefined) =>
  (v ?? "")
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

export function iosAppIds(): string[] {
  return list(process.env.FEAST_IOS_APP_IDS).filter((id) => /^[A-Z0-9]{10}\.[A-Za-z0-9.-]+$/.test(id));
}

export interface AndroidApp {
  packageName: string;
  fingerprints: string[];
}

export function androidApps(): AndroidApp[] {
  const out: AndroidApp[] = [];
  for (const entry of list(process.env.FEAST_ANDROID_APPS)) {
    const i = entry.indexOf(":");
    if (i <= 0) continue;
    const packageName = entry.slice(0, i).trim();
    const fingerprints = entry
      .slice(i + 1)
      .split("|")
      .map((f) => f.trim().toUpperCase())
      .filter((f) => /^([0-9A-F]{2}:){31}[0-9A-F]{2}$/.test(f));
    if (/^[A-Za-z][\w]*(\.[A-Za-z][\w]*)+$/.test(packageName) && fingerprints.length) out.push({ packageName, fingerprints });
  }
  return out;
}

/** apple-app-site-association (served as JSON, no extension, no redirect). */
export function appleAppSiteAssociation(appIds = iosAppIds()) {
  return {
    applinks: {
      details: [
        {
          appIDs: appIds,
          components: [...EXCLUDED_PATHS.map((p) => ({ "/": p, exclude: true, comment: "API and admin stay on the web" })), { "/": "*" }],
        },
      ],
    },
    webcredentials: { apps: appIds },
  };
}

/** Digital Asset Links for Android App Links. */
export function assetLinks(apps = androidApps()) {
  return apps.map((a) => ({
    relation: ["delegate_permission/common.handle_all_urls", "delegate_permission/common.get_login_creds"],
    target: { namespace: "android_app", package_name: a.packageName, sha256_cert_fingerprints: a.fingerprints },
  }));
}
