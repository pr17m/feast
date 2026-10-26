# Feast — web app

Dine-out platform MVP: discover restaurants, pre-book tables with slot deals, tell the restaurant you've arrived, and pay the bill with instant discounts (the restaurant's QR code opens its page). It has three roles: **customer**, **restaurant owner** (partner portal) and **admin** (full control).

Everything runs in the browser against a **mock backend** (seeded data saved in `localStorage`), behind the same typed API contract the future Go backend will implement. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Run it

```bash
npm install
npm run dev          # http://localhost:3000
```

Production build: `npm run build && npm start`.

## Demo accounts (one-tap buttons on /login)

| Role | Sign in | Lands on |
|---|---|---|
| Customer | mobile `9876543210`, OTP `123456` (any valid number works and creates a new diner) | `/` |
| Owner — Olive & Ember, Hops Republic (Bengaluru) | `owner@feast.demo` / `feast123` | `/partner` |
| Owner — Bombay Monsoon, Salt Water Social (Mumbai) | `owner.mumbai@feast.demo` / `feast123` | `/partner` |
| Admin | `admin@feast.demo` / `feast123` | `/admin` |

**Try the full loop.**

1. As a customer, book a table for today, then open **At the restaurant** and tap **I'm here**.
2. Sign in as the owner. The booking shows as arrived on **Bookings**; print the restaurant's code from **QR code**, and mark *Burrata* out of stock under **Menu**.
3. Sign back in as the customer. The item now shows as unavailable. Tap **Pay bill & get your deal**, type the amount from the restaurant's bill, and pay through the mock checkout (you can simulate a declined payment) or choose to pay at the counter.
4. As admin, onboard a restaurant, approve it, suspend another, or change user roles.

Admin → **Reset demo data** restores the seed.

## Switch to a real backend

```bash
NEXT_PUBLIC_API_MODE=http NEXT_PUBLIC_API_BASE_URL=http://localhost:8080 npm run dev
```

`src/lib/api/http.ts` calls the REST routes listed in `src/lib/api/client.ts`.

## Install as an app (no store needed)

Feast is an installable web app, alongside the iOS/Android store apps:

- **Two apps from one site.** "Feast" (diners, scope `/`, manifest `/manifest.webmanifest`) and "Feast Partner" (restaurants, scope `/partner`, manifest `/partner/manifest.webmanifest`). The admin area links no manifest, so it isn't installable. Manifests are built in `src/lib/pwa-manifest.ts`; icons come from `mobile/shared/assets` via `python3 webapp/scripts/pwa-icons.py` → `public/icons/`.
- **Android, Chrome and Edge (desktop too).** The browser's install prompt is kept (inline script in the root layout) and offered as an "Install" card on the At the restaurant page and a confirmed booking (diners), and on the partner dashboard and sidebar ("Install app"). "Not now" hides it for 60 days. Users can always install from the browser menu.
- **iPhone/iPad.** Safari has no install prompt, so the same cards open "Share → Add to Home Screen" steps. Hidden once opened from the home screen, and inside the store apps.
- **Service worker** (`/sw.js`, source in `src/lib/service-worker.ts`, production builds only, never inside the store apps). Static assets are cache-first in a cache named after the build; pages are network-first, with the last visited copy kept for offline use and `/offline` for anything else. Never cached: `/v1/*` (all personal data), `runtime-config.js`, the worker, `/admin`, sign-in, payment and receipt pages, QR links, `/.well-known`, and Next's client data requests. A new deploy shows "New version of Feast available — Refresh".
- **Not yet:** web push for installed apps (iOS 16.4+ supports it for home-screen apps; the server's FCM sender could be reused later). HTTPS is required for installation and the service worker (localhost is exempt).

## Tests

`npm run test:e2e` drives every role at desktop and phone sizes with Playwright and saves screenshots to `e2e/shots/`. Start the app first (`npm run build && npm start -- -p 3100`), and install the browser once with `npx playwright install chromium`.

## Tech

Next.js 16 (App Router) · React 19 · TypeScript (strict) · Tailwind CSS 4 · lucide icons · qrcode. No backend dependencies yet.
