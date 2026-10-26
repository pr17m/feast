/**
 * Web app manifests for "Add to Home Screen" / "Install app" (Chrome, Edge,
 * Android, iOS Safari). Two install identities on one site:
 *
 *   Feast          diner app, scope "/"         → /manifest.webmanifest
 *   Feast Partner  restaurant app, scope "/partner" → /partner/manifest.webmanifest
 *
 * The admin area isn't installable (its pages link no manifest).
 */

const THEME = "#0F766E";
const BACKGROUND = "#FAF7F2";

type App = "diner" | "partner";

function icons(app: App) {
  return [
    { src: `/icons/${app}-192.png`, sizes: "192x192", type: "image/png", purpose: "any" },
    { src: `/icons/${app}-512.png`, sizes: "512x512", type: "image/png", purpose: "any" },
    { src: `/icons/${app}-maskable-192.png`, sizes: "192x192", type: "image/png", purpose: "maskable" },
    { src: `/icons/${app}-maskable-512.png`, sizes: "512x512", type: "image/png", purpose: "maskable" },
  ];
}

const shortcutIcon = (app: App) => [{ src: `/icons/${app}-96.png`, sizes: "96x96", type: "image/png" }];

export function manifestFor(app: App) {
  if (app === "partner") {
    return {
      id: "/partner?app=feast-partner",
      name: "Feast Partner",
      short_name: "Feast Partner",
      description: "Run your restaurant on Feast: bookings, counter payments, offers and insights.",
      start_url: "/partner?source=pwa",
      scope: "/partner",
      display: "standalone",
      orientation: "any",
      theme_color: THEME,
      background_color: BACKGROUND,
      categories: ["business", "food"],
      icons: icons("partner"),
      shortcuts: [
        { name: "Bookings", short_name: "Bookings", url: "/partner/bookings", icons: shortcutIcon("partner") },
        { name: "Payments to confirm", short_name: "Payments", url: "/partner/payments", icons: shortcutIcon("partner") },
      ],
    };
  }
  return {
    id: "/?app=feast",
    name: "Feast",
    short_name: "Feast",
    description: "Book a table, get the restaurant's deal and pay the bill on Feast.",
    start_url: "/?source=pwa",
    scope: "/",
    display: "standalone",
    orientation: "any",
    theme_color: THEME,
    background_color: BACKGROUND,
    categories: ["food", "lifestyle"],
    icons: icons("diner"),
    shortcuts: [
      { name: "My bookings", short_name: "Bookings", url: "/bookings", icons: shortcutIcon("diner") },
      { name: "At the restaurant", short_name: "Pay bill", url: "/dine-in", icons: shortcutIcon("diner") },
      { name: "My savings", short_name: "Savings", url: "/account/insights", icons: shortcutIcon("diner") },
    ],
  };
}

export function manifestResponse(app: App): Response {
  return new Response(JSON.stringify(manifestFor(app)), {
    headers: { "content-type": "application/manifest+json; charset=utf-8", "cache-control": "public, max-age=3600" },
  });
}
