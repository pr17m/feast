# Feast backend

| Folder              | What                                                                                     |
|---------------------|------------------------------------------------------------------------------------------|
| `rust/`             | **Production API.** Cargo workspace, one binary `feast` with roles                       |
| `go/`               | Go port scaffold and `GO_PORTING_PLAN.md`                                                |
| `config/`           | `default.toml` (every setting, documented), env overrides, `countries/<cc>.yaml`         |
| `migrations/`       | SQL migrations per database: `sqlite/` (development), `postgres/` (production)           |
| `protos/`           | gRPC contracts (notification service), shared by the Rust and Go backends                |
| `seed/demo.json`    | Development demo data (filtered to the deployment's country)                             |
| `spec/`             | `openapi.yaml` (generated), `conformance/api_flow.py`                                    |
| `Dockerfile`        | One image for every country and role                                                     |

## Run locally

```sh
cd rust
FEAST_COUNTRY=in cargo run                  # India on :8080, SQLite in data/in/, demo data
FEAST_COUNTRY=au cargo run -- --help        # roles: all | gateway | worker | notification | migrate | seed | markets sync | config
```

A country is required. There is deliberately no default, so a deployment can
never start as the wrong country.

## Layers (Rust crates)

```
domain        pure rules: money, tax, price bands, slots, subscriptions, compliance, events (no I/O)
application   use cases on `Feast` + ports (traits) for everything external
infra         adapters: sql (SQLite/Postgres), bus (memory/Redis Streams), payments, storage,
              otp, oauth, geo, clock, ids, seed
notification  notification service (embedded or gRPC) with its own store
api           axum HTTP + SSE; thin: parse → use case → JSON ({code,message} errors)
server        composition root: config → adapters → roles
```

The rules that keep the code swappable (SOLID):

- **Dependency inversion.** Use cases depend on `ports::*` traits only.
  `server/src/wiring.rs` is the only place that picks implementations, based on
  config.
- **Open/closed.** New providers are new adapter files. For example, payments:
  1. Write `infra/src/payments/<provider>.rs` implementing `PaymentGateway`.
  2. Add one line to `FACTORIES` in `infra/src/payments/mod.rs`.
  3. Give it a config section `[payments.providers.<provider>]`.
  4. Set `payments.provider: <provider>` in a country file.

  Nothing else changes. India uses `razorpay` and Australia uses `stripe` this
  way.
- **Single responsibility.** Each module covers one bounded context:
  `booking` (incl. arrival check-in), `qr`, `billing`, `subscriptions`, `compliance`, `markets`,
  `admin`, `partner`, `auth`, `notifications`.
- **Interface segregation.** Small ports: `MarketRepository`,
  `SubscriptionRepository`, `ComplianceRepository`, `FileStorage`,
  `OtpService`, `GeoLocator`…

## Swappable technology

| Concern          | Setting                                  | Options                                                                    |
|------------------|------------------------------------------|----------------------------------------------------------------------------|
| Database         | `database.url`                           | `sqlite://…`, `postgres://…` (RDS, Azure Flexible Server, Cloud SQL socket) |
| Message bus      | `bus.kind`                               | `memory` (one process), `redis` (Streams + consumer groups; `rediss://` TLS) |
| Notifications    | `notifications.mode`                     | `embedded`, `grpc` (separate `feast notification` service)                  |
| Payments         | country file + `[payments.providers.*]`  | `mock` (test), `razorpay`, `stripe`                                         |
| Documents        | `storage.kind`                           | `local`, `s3` (AWS S3, Google Cloud Storage, R2, MinIO), `azure`            |
| Diner OTP        | `auth.otp.provider`                      | `fixed` (dev), `log`, `twilio` (Twilio Verify)                              |
| Social login     | `auth.oauth.{google,apple,facebook}.mode`| `off`, `demo` (dev), `live`                                                 |
| Visitor country  | `geo`                                    | CDN headers, MaxMind GeoLite2                                               |

## Configuration

Settings are layered, with later layers winning:

1. `config/default.toml`
2. `config/<env>.toml`
3. `config/countries/<cc>.toml`
4. `config/local.toml`, git-ignored, for secrets
5. `FEAST__SECTION__KEY` environment variables

`feast config` prints the effective settings with secrets masked.

`FEAST_ENV=production` refuses unsafe settings (test payments, fixed OTP,
default secrets, wildcard CORS…) and lists what to fix.

## Events and live updates

State changes write domain events to an **outbox** in the same transaction.
The worker relays them to the bus (Redis Streams). Consumers are idempotent:

- **Notification projector:** turns events into notifications, e.g. a new
  booking notifies the restaurant's owners.
- **Gateways:** push to browsers over SSE at `/v1/me/events`.

Run as many gateways and workers as you like; side effects still happen once:

- **One relay publishes at a time.** Workers take the `outbox-relay` lease in
  the database (one atomic conditional upsert; renewed every tick, 15 s TTL),
  so rows are published in order and once. If the holder dies, another worker
  takes over when the lease expires.
- **Consumers are idempotent by event id.** Notifications are unique per
  `(event_id, user_id)`; each notification's push is claimed once per phone
  (`push_deliveries`), so a sent push is never repeated, while a passing FCM
  failure (5xx, 429, timeout) is retried by the sweeper with backoff (30 s,
  1 min, 2 min…; 5 attempts, then given up and logged) and an uninstalled
  app's token is dropped; audit records are written with the event in the
  outbox transaction (record id `<event id>#<n>`).
- **State-checked changes.** Cancelling a booking (diner, restaurant, admin,
  account deletion) is a conditional update from its current status: of two
  at once, one wins, so there is one notification, audit record and refund.
- **One sweeper at a time** (`sweeper` lease): reminders, overdue cases,
  unconfirmed counter payments, unsent refunds, push retries and expired
  rate-limit/OTP rows.

The reminder sweeper sends subscription-expiry reminders (7, 3 and 1 days
before) and licence-expiry reminders (30, 7 and 1 days before).

## Payments and refunds

Money moves only through the payment provider (Stripe in Australia, Razorpay
in India, `mock` in development). Every settlement path is idempotent and
checks state in the same transaction:

- A payment confirms its booking only while the booking still waits for it
  (and, if its 20-minute hold lapsed, a table is still free). A payment that
  arrives later — booking cancelled or expired, table taken, booking already
  paid, bill already paid online or at the counter — never changes the booking
  or bill: it is recorded as not applied and refunded in full, once.
- **Refunds** (`refunds` table): created PENDING with an idempotency key (one
  per reason, e.g. `booking-cancel:<id>`), sent to the provider with the refund
  id as its idempotency key/receipt, and settled by the provider's answer or
  its webhook (`refund.*` on Stripe, `refund.processed/failed` on Razorpay).
  Final states stay final; unsent refunds are retried by the sweeper (up to 8
  attempts, then FAILED and admins are told).
- When: a diner cancels at least 2 hours before the slot, the restaurant or
  Feast cancels, the diner deletes their account (same 2-hour rule), a late or
  double payment, an admin's decision on a complaint ("refund advised"), or an
  admin refund by hand (`POST /v1/admin/refunds`, with a note, never more than
  what is left).
- Counter (offline) payments never pass through Feast, so Feast never refunds
  them; the restaurant does.

**Typed bills.** The diner types the restaurant's final bill total, taxes
included (GST in both Australia and India); Feast adds no tax on top and shows
the line as "Bill total (incl. GST)". A deal applies to that total unless the
offer's own terms say otherwise.

## Production notes

- **Health:**
  - `/healthz` (liveness)
  - `/readyz` (checks the database, bus, notifications and storage)
  - `/metrics` (Prometheus; protect it with `http.metrics_token`)
- **Rate limits:** sign-in, OTP and signup are limited per client IP. Set
  `http.proxy_hops` to the number of proxies in front of the API so the real
  IP is used. Counters (fixed one-minute windows) and OTP challenges (a hash of
  the code, attempts, expiry) live in the database, so limits and codes hold
  across every gateway instance; the sweeper deletes expired rows. If the
  database can't be reached the limiter lets requests through (sign-in fails
  anyway without it).
- **Uploads:** documents are type-sniffed (PDF/JPG/PNG/WebP only), size-capped,
  and served only to the restaurant's owners and admins, with `nosniff` and
  `sandbox`.
