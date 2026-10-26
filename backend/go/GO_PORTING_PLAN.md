# Porting the Feast API to Go

The **Rust backend (`../rust`) is the production backend**. This folder is a
runnable Go scaffold, plus the plan to reach feature parity when you want to
switch.

## Why a port is low-risk

- **One contract.** `../spec/openapi.yaml` is generated from the Rust routes and
  the shared TypeScript types. The Go server serves the same paths, JSON shapes
  and `{code, message}` errors.
- **One database schema.** The Go server uses `../migrations/postgres` as is.
  A Go API can take over an existing country database without data migration.
- **One configuration.** Same environment variables (`FEAST_COUNTRY`,
  `FEAST__DATABASE__URL`, `FEAST__BUS__REDIS__URL`…) and the same
  `config/countries/<cc>.yaml` files.
- **One deployment shape.** The web app proxies `/v1/*` to whatever answers on
  `:8080`. Swapping the Rust image for the Go image in the Terraform/compose
  stack is the whole cutover.
- **Same tests.** `../spec/conformance/api_flow.py` (HTTP) and
  `webapp/e2e/smoke.mjs` (browser) run unchanged against either backend.

## What's here

| Path                   | Status                                                                                                                                          |
|------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------|
| `internal/domain`      | **Ported, with the Rust test cases:** money formatting (Indian grouping), price bands, plans/tax, E.164 phones, subscription lifecycle, invoice numbers, compliance rules |
| `internal/ports`       | Interfaces mirroring `feast_application::ports` (repositories, bus, payment gateways, storage, OTP, clock)                                      |
| `internal/config`      | Same env vars as Rust                                                                                                                           |
| `internal/httpapi`     | Router, JSON/error helpers, `/healthz`, `/readyz`. Unported `/v1/*` routes return `501 NOT_IMPLEMENTED`, so conformance runs show what's left   |
| `cmd/feast`            | Entrypoint with graceful shutdown                                                                                                               |
| `Dockerfile`           | Static distroless image, drop-in for `feast-api`                                                                                                |

```sh
go test ./...
FEAST_COUNTRY=au go run ./cmd/feast      # :8080
```

## Crate → package map and suggested libraries

| Rust crate                         | Go package                     | Libraries                                                                                                      |
|------------------------------------|--------------------------------|----------------------------------------------------------------------------------------------------------------|
| `domain`                           | `internal/domain`              | stdlib                                                                                                         |
| `application` (use cases + ports)  | `internal/app`, `internal/ports` | stdlib                                                                                                       |
| `infra/sql`                        | `internal/adapters/postgres`   | `jackc/pgx/v5` (pool), `pressly/goose` or `golang-migrate` pointed at `../migrations/postgres`                  |
| `infra/bus/redis`                  | `internal/adapters/redisbus`   | `redis/go-redis/v9`: same stream (`feast:events`), consumer group, XAUTOCLAIM                                   |
| `infra/payments/{stripe,razorpay}` | `internal/adapters/payments`   | `stripe/stripe-go/v81`; Razorpay via `net/http` (orders + HMAC signature)                                       |
| `infra/storage`                    | `internal/adapters/storage`    | `aws/aws-sdk-go-v2/service/s3` (S3 + GCS interop), `Azure/azure-sdk-for-go/sdk/storage/azblob`                  |
| `infra/otp` (Twilio Verify)        | `internal/adapters/otp`        | `net/http`                                                                                                     |
| `infra/oauth`                      | `internal/adapters/oauth`      | `coreos/go-oidc/v3` (Google/Apple), Facebook Graph via `net/http`                                               |
| `api` (axum)                       | `internal/httpapi`             | `net/http` 1.22+ routing (already used); SSE via `http.Flusher`                                                 |
| `notification` (gRPC)              | `internal/notification`        | `google.golang.org/grpc`; generate from `../protos` with `buf generate` (`../protos/buf.gen.go.yaml`)           |
| `server` (roles, config)           | `cmd/feast`, `internal/config` | `BurntSushi/toml`, `gopkg.in/yaml.v3`                                                                           |

## Order of work (each step ships behind the same tests)

1. **Read paths:** markets, cities, geo, restaurants, search, menu, slots. Postgres adapter for those tables.
2. **Auth:** OTP (Twilio + fixed dev code), password login, JWT (same secret and claims, so tokens work across backends), social login.
3. **Bookings + check-in + restaurant QR + bills + payments** (mock, Stripe, Razorpay), with the transactional outbox.
4. **Bus + worker:** outbox relay, notification projector (Redis consumer group), reminder sweeper, SSE `/v1/me/events`.
5. **Partner + admin:** onboarding documents (storage adapters), approvals, subscriptions, markets admin.
6. **Notification service over gRPC**, if you run it separately.

**Done means:** `api_flow.py` and the browser suite pass against the Go image.
Then switch one country's API image, watch `/metrics` and the logs, and roll
the others.

## Rules to keep

- **Money:** integer minor units, never floats. Round tax with the same functions (`TaxOn`).
- **Times:** stored as RFC 3339 UTC strings. "Today" and slot times use the city's time zone.
- **Events:** at-least-once delivery. Consumers stay idempotent; notifications are keyed by event id.
- **Visibility:** never list a restaurant unless it is LIVE/PAUSED, compliance is complete, the subscription is listed, and the market is LIVE.
