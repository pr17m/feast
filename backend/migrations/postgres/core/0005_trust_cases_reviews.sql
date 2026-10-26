-- Trust & safety: staff-only restaurant fields, grievance cases, reviews, audit trail.

-- Staff-only restaurant fields (support access window, pre-Feast rating base), JSON.
ALTER TABLE restaurants ADD COLUMN extras TEXT;

-- Grievances between a diner and a restaurant about one visit.
CREATE TABLE cases (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,          -- COMPLAINT | REPORT
    status          TEXT NOT NULL,
    restaurant_id   TEXT NOT NULL,
    customer_id     TEXT NOT NULL,
    booking_id      TEXT,
    bill_payment_id TEXT,
    due_at          TEXT,
    version         BIGINT NOT NULL DEFAULT 1,
    data            TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX cases_restaurant ON cases (restaurant_id, created_at);
CREATE INDEX cases_customer ON cases (customer_id, created_at);
CREATE INDEX cases_status_due ON cases (status, due_at);
-- One open case per visit per side (enforced in the app; this speeds the lookup).
CREATE INDEX cases_visit ON cases (kind, booking_id, bill_payment_id);

-- Verified reviews: one per visit.
CREATE TABLE reviews (
    id              TEXT PRIMARY KEY,
    restaurant_id   TEXT NOT NULL,
    user_id         TEXT NOT NULL,
    visit_key       TEXT NOT NULL,          -- "booking:<id>" or "bill:<id>"
    status          TEXT NOT NULL,          -- PUBLISHED | HIDDEN
    rating          BIGINT NOT NULL,
    flagged         BIGINT NOT NULL DEFAULT 0,
    data            TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    UNIQUE (visit_key)
);
CREATE INDEX reviews_restaurant ON reviews (restaurant_id, status, created_at);
CREATE INDEX reviews_user ON reviews (user_id, created_at);

-- Every action by Feast staff on a restaurant (and owners granting access).
CREATE TABLE audit_log (
    id              TEXT PRIMARY KEY,
    restaurant_id   TEXT NOT NULL,
    data            TEXT NOT NULL,
    at              TEXT NOT NULL
);
CREATE INDEX audit_log_restaurant ON audit_log (restaurant_id, at);
