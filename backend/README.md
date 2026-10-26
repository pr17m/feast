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

Delivery is at least once.

The reminder sweeper sends subscription-expiry reminders (7, 3 and 1 days
before) and licence-expiry reminders (30, 7 and 1 days before).

## Production notes

- **Health:**
  - `/healthz` (liveness)
  - `/readyz` (checks the database, bus, notifications and storage)
  - `/metrics` (Prometheus; protect it with `http.metrics_token`)
- **Rate limits:** sign-in, OTP and signup are limited per client IP. Set
  `http.proxy_hops` to the number of proxies in front of the API so the real
  IP is used.
- **Uploads:** documents are type-sniffed (PDF/JPG/PNG/WebP only), size-capped,
  and served only to the restaurant's owners and admins, with `nosniff` and
  `sandbox`.
