# Feast

Discover restaurants, pre-book tables with deals, and pay the bill on Feast to
get the deal. Restaurant owners manage their listing, menu, bookings and
subscription. Admins onboard restaurants per country and run the platform.

**Launch countries:** India 🇮🇳 and Australia 🇦🇺. Next: New Zealand, Canada, UK.

| Folder        | What                                                                                                         |
|---------------|--------------------------------------------------------------------------------------------------------------|
| `webapp/`     | Next.js web app: diners, restaurant partners and admins; phone-friendly                                      |
| `backend/`    | Production API in **Rust** (`rust/`), Go port scaffold (`go/`), DB migrations, gRPC protos, API spec, country configs |
| `deploy/`     | One-command deploys: Docker (laptop/VM), AWS, Azure, Google Cloud                                            |
| `.github/`    | CI: tests (SQLite + Postgres), browser tests for both countries, multi-arch images                           |

## How it fits together

```
 web (Next.js) ──/v1──▶ API (Rust) ──▶ PostgreSQL       one stack per country,
   the only public       │  worker      Redis (events)    same code + images,
   endpoint              └──────────▶ file storage      FEAST_COUNTRY=au|in|…
```

- **Every country is its own deployment:** its own database, Redis and files.
  All countries run the same commit and images. The country is picked at
  deploy time, and everything country-specific lives in
  `backend/config/countries/<cc>.yaml`:
  - currency, tax, prices
  - payment provider (Razorpay for India, Stripe for Australia)
  - phone format
  - licence documents
  - cities
- **Architecture:** ports & adapters, SOLID. Business code depends on
  interfaces. Database (SQLite/Postgres), message bus (memory/Redis), payments
  (test/Razorpay/Stripe), storage (disk/S3/GCS/Azure Blob), OTP (dev/Twilio)
  and social login are adapters chosen by config. Adding Stripe for a new
  country, or a new provider entirely, never touches business code.
  See `backend/README.md`.
- **Restaurants go live only when** the admin has approved their licence
  documents, their subscription is active, and the country is live. Each
  restaurant gets a 30-day free trial, then a monthly, quarterly or yearly plan.

## Run it

| Goal                          | How                                                                                       |
|-------------------------------|-------------------------------------------------------------------------------------------|
| Try it on your machine        | `deploy/compose/README.md`: Docker, one command, optional public share link               |
| Deploy to AWS / Azure / GCP   | `deploy/README.md`: `./deploy.sh au` per country                                          |
| Develop the backend           | `cd backend/rust && FEAST_COUNTRY=in cargo run` (SQLite + in-memory bus, demo data)       |
| Develop the web app           | `cd webapp && npm install && FEAST_API_INTERNAL_URL=http://localhost:8080 npm run dev`    |

Development sign-in for the demo data:
- Diners: any phone number valid in that country, OTP `123456`.
- `owner@feast.demo` (India) and `owner.au@feast.demo` (Australia), password `feast123`.
- `admin@feast.demo`, password `feast123`.

Deployed stacks generate their own password; see the deploy docs.

## Tests

```sh
cd backend/rust && cargo test --workspace                 # domain, use cases, adapters
python3 backend/spec/conformance/api_flow.py http://localhost:8080   # API contract (any backend)
cd webapp && node e2e/smoke.mjs                           # browser: India :3100 + Australia :3101
```

API contract: `backend/spec/openapi.yaml`, generated from the code
(`backend/spec/generate.py`).
