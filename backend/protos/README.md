# Feast protos

Internal service-to-service APIs (gRPC). Public client APIs stay REST/JSON (see `../docs/openapi.yaml`).

```
protos/
  feast/common/v1/common.proto              Caller, pagination
  feast/notification/v1/notification.proto  NotificationService
```

Rules:
- One package per service and major version: `feast.<service>.v<N>`. Breaking changes need a new `v<N+1>` package.
- Never reuse or renumber a field; reserve removed ones (`reserved 4;`).
- Enums start with `<ENUM>_UNSPECIFIED = 0`.
- Every request carries `feast.common.v1.Caller` for auth context and tracing.

Rust compiles these at build time (`crates/proto/build.rs`, pure Rust via `protox` — no `protoc` needed).
Go: `buf generate --template buf.gen.go.yaml`.
