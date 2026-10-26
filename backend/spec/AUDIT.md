# Audit trail

Every state change in Feast leaves an immutable record in `audit_trail`: who did
it, what changed, from which status to which, for how much, and when.

## How records are written

* **From domain events (most changes).** Bookings, payments, deals,
  disputes, reviews and compliance changes already emit a domain event, written
  to the outbox in the same database transaction as the change. `insert_events`
  (infra) writes the event's audit records (`feast_domain::audit::records_for`)
  in that same transaction, so the trail can't miss a committed change or keep
  one that rolled back, and a replayed event can't duplicate a record
  (record id = `<event id>#<n>`).
* **Money going back:** a payment that couldn't be applied (`payment.not_applied`),
  and every refund step (`refund.requested`, `refund.succeeded`,
  `refund.failed`; entity `refund`, with the reason, payment and booking/bill
  in the note). Admin refunds and case decisions are by the admin; automatic
  ones by the diner/restaurant who cancelled, the provider's webhook or
  `Feast · sweeper`.
* **Directly, for changes without an event:** owner/staff content and settings
  edits, admin user and restaurant status changes, documents uploaded (`Feast::audit_direct`).
* **Append-only:** `AuditTrailRepository` has only `append` and `query`; nothing
  updates or deletes records (only the dev "reset demo data" wipe clears them).

## Who did it

The API opens an actor scope per request (request id + client IP); signing in
fills in the user (id, role, name). Provider webhooks are recorded as the
provider; background jobs as `Feast · sweeper`. Every event built inside the
scope carries the actor (`EventEnvelope.actor`); see
`rust/crates/application/src/actor.rs`.

## Record

```json
{ "id": "evt_…#0", "at": "2026-10-25T08:00:00.000Z", "restaurantId": "r_…",
  "entityType": "offline_payment", "entityId": "op_…", "action": "payment.offline_approved",
  "actor": { "kind": "USER", "userId": "u_…", "role": "OWNER", "name": "…", "ip": "…", "requestId": "…" },
  "subjectUserId": "u_diner", "fromStatus": "PENDING_APPROVAL", "toStatus": "APPROVED",
  "amount": 4500, "currency": "AUD", "note": "CASH · bill bill_…", "eventId": "evt_…" }
```

## Reading it

| Who | Endpoint | Scope |
| --- | --- | --- |
| Admin | `GET /v1/admin/audit?restaurantId&entityType&entityId&actorId&userId&from&to&cursor&limit` | everything |
| Owner / staff | `GET /v1/partner/restaurants/{id}/audit?…` | that restaurant |
| Diner | `GET /v1/bookings/{id}/history`, `GET /v1/bill-payments/{id}/history` | their own booking / payment |

Newest first, keyset-paginated (`nextCursor`), at most 200 per page.
