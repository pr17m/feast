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

## Tests

`npm run test:e2e` drives every role at desktop and phone sizes with Playwright and saves screenshots to `e2e/shots/`. Start the app first (`npm run build && npm start -- -p 3100`), and install the browser once with `npx playwright install chromium`.

## Tech

Next.js 16 (App Router) · React 19 · TypeScript (strict) · Tailwind CSS 4 · lucide icons · qrcode. No backend dependencies yet.
