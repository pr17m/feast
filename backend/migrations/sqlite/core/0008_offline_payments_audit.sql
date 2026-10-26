-- Counter (offline) payments: a diner pays a Feast bill in cash or on the
-- restaurant's own terminal; the owner confirms it. One open (PENDING_APPROVAL)
-- payment per bill. Keep in sync with the other dialect's 0008.
CREATE TABLE offline_payments (
    id            TEXT PRIMARY KEY,
    bill_id       TEXT NOT NULL,
    intent_id     TEXT NOT NULL,
    restaurant_id TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    status        TEXT NOT NULL,
    submitted_at  TEXT NOT NULL,
    expires_at    TEXT NOT NULL,
    data          TEXT NOT NULL             -- JSON OfflinePayment
);
CREATE INDEX offline_payments_restaurant ON offline_payments (restaurant_id, status, submitted_at);
CREATE INDEX offline_payments_bill ON offline_payments (bill_id, submitted_at);
CREATE INDEX offline_payments_due ON offline_payments (status, expires_at);
CREATE UNIQUE INDEX offline_payments_one_open ON offline_payments (bill_id) WHERE status = 'PENDING_APPROVAL';

-- The audit trail: append-only, one row per state change (written in the same
-- transaction as the change; see feast_domain::audit). Never updated or deleted
-- by the application.
CREATE TABLE audit_trail (
    id              TEXT PRIMARY KEY,       -- <event id>#<n>, or aud_… for direct records
    at              TEXT NOT NULL,
    restaurant_id   TEXT,
    entity_type     TEXT NOT NULL,
    entity_id       TEXT NOT NULL,
    action          TEXT NOT NULL,
    actor_kind      TEXT NOT NULL,
    actor_id        TEXT,
    subject_user_id TEXT,
    event_id        TEXT,
    data            TEXT NOT NULL           -- JSON AuditRecord
);
CREATE INDEX audit_trail_at ON audit_trail (at, id);
CREATE INDEX audit_trail_restaurant ON audit_trail (restaurant_id, at, id);
CREATE INDEX audit_trail_entity ON audit_trail (entity_type, entity_id, at);
CREATE INDEX audit_trail_entity_id ON audit_trail (entity_id, at);
CREATE INDEX audit_trail_actor ON audit_trail (actor_id, at);
CREATE INDEX audit_trail_subject ON audit_trail (subject_user_id, at);
