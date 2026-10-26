/**
 * Source of the service worker served at /sw.js (see app/sw.js/route.ts).
 *
 * What it does, and deliberately doesn't:
 * - Static assets (/_next/static, /icons, the manifests): cache-first, in a
 *   cache named after the build, so every deploy starts clean.
 * - Pages: network-first; the last copy of each visited page is kept so it can
 *   open offline, and anything else offline shows /offline.
 * - Never touched (straight to the network, never stored): the API (/v1/*),
 *   runtime-config.js, the worker itself, the admin area, sign-in, payment and
 *   receipt pages, QR short links, /.well-known, health checks, and Next's
 *   client-side data requests (RSC). Personal data only ever comes from /v1,
 *   so nothing personal is cached.
 * - Updates: a new worker waits until the page asks it to take over (the
 *   "New version available" banner), then the page reloads once.
 *
 * Kept as plain ES2017 with no template literals so it can live in a string.
 */
export const SERVICE_WORKER_SOURCE = String.raw`/* Feast service worker */
"use strict";
var BUILD = __FEAST_BUILD__;
var STATIC_CACHE = "feast-static-" + BUILD;
var PAGES_CACHE = "feast-pages-" + BUILD;
var OFFLINE_URL = "/offline";
var MAX_PAGES = 40;
var PRECACHE = [
  OFFLINE_URL,
  "/manifest.webmanifest",
  "/partner/manifest.webmanifest",
  "/icons/diner-192.png",
  "/icons/partner-192.png"
];
var NEVER = [
  "/v1/", "/runtime-config.js", "/sw.js", "/admin", "/login", "/pay/", "/payments",
  "/r/", "/t/", "/.well-known/", "/healthz", "/api/"
];

function bypass(path) {
  for (var i = 0; i < NEVER.length; i++) {
    if (path === NEVER[i] || path.indexOf(NEVER[i]) === 0) return true;
  }
  return false;
}

function isStatic(path) {
  return path.indexOf("/_next/static/") === 0 || path.indexOf("/icons/") === 0 ||
    path === "/favicon.ico" || /\.webmanifest$/.test(path);
}

self.addEventListener("install", function (event) {
  event.waitUntil(caches.open(STATIC_CACHE).then(function (cache) {
    return cache.addAll(PRECACHE.map(function (u) { return new Request(u, { cache: "reload" }); }));
  }));
});

self.addEventListener("activate", function (event) {
  event.waitUntil(caches.keys().then(function (keys) {
    return Promise.all(keys.map(function (k) {
      if (k.indexOf("feast-") === 0 && k !== STATIC_CACHE && k !== PAGES_CACHE) return caches.delete(k);
      return null;
    }));
  }).then(function () { return self.clients.claim(); }));
});

self.addEventListener("message", function (event) {
  if (event.data && event.data.type === "SKIP_WAITING") self.skipWaiting();
});

function trim(cache) {
  return cache.keys().then(function (keys) {
    if (keys.length <= MAX_PAGES) return null;
    return Promise.all(keys.slice(0, keys.length - MAX_PAGES).map(function (k) { return cache.delete(k); }));
  });
}

function cacheFirst(request) {
  return caches.open(STATIC_CACHE).then(function (cache) {
    return cache.match(request).then(function (hit) {
      if (hit) return hit;
      return fetch(request).then(function (res) {
        if (res.ok && res.type === "basic") cache.put(request, res.clone());
        return res;
      });
    });
  });
}

function networkFirstPage(request) {
  return fetch(request).then(function (res) {
    if (res.ok && res.type === "basic" && !res.redirected) {
      var copy = res.clone();
      caches.open(PAGES_CACHE).then(function (cache) { return cache.put(request, copy).then(function () { return trim(cache); }); });
    }
    return res;
  }).catch(function () {
    return caches.open(PAGES_CACHE).then(function (cache) { return cache.match(request); }).then(function (hit) {
      return hit || caches.match(OFFLINE_URL);
    }).then(function (res) {
      return res || new Response("You're offline.", { status: 503, headers: { "content-type": "text/plain; charset=utf-8" } });
    });
  });
}

self.addEventListener("fetch", function (event) {
  var request = event.request;
  if (request.method !== "GET") return;
  var url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  var path = url.pathname;
  if (bypass(path)) return;
  // Next's client-side navigations and prefetches carry live data: network only.
  if (request.headers.get("RSC") === "1" || url.searchParams.has("_rsc")) return;
  if (isStatic(path)) { event.respondWith(cacheFirst(request)); return; }
  if (request.mode === "navigate") { event.respondWith(networkFirstPage(request)); return; }
});
`;
