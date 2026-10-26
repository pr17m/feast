-- PostgreSQL version of migrations/sqlite/core/0001_init.sql (keep both in sync).
-- Feast core database (PostgreSQL). Portable SQL: TEXT/BIGINT only, JSON stored as TEXT.
-- Same schema as migrations/sqlite/core with Postgres types (BIGINT, BIGSERIAL).
-- Timestamps are RFC 3339 UTC strings (sortable); dates are YYYY-MM-DD local.

-- ---------------------------------------------------------------- identity
CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    phone         TEXT UNIQUE,
    email         TEXT UNIQUE,              -- stored lower-cased
    role          TEXT NOT NULL CHECK (role IN ('CUSTOMER', 'OWNER', 'ADMIN')),
    status        TEXT NOT NULL CHECK (status IN ('ACTIVE', 'BLOCKED')),
    password_hash TEXT,                     -- argon2id PHC string; NULL for OTP-only users
    created_at    TEXT NOT NULL
);

-- Which restaurants an OWNER manages (many-to-many).
CREATE TABLE user_restaurants (
    user_id       TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    restaurant_id TEXT NOT NULL,
    PRIMARY KEY (user_id, restaurant_id)
);
CREATE INDEX user_restaurants_restaurant ON user_restaurants (restaurant_id);

-- ---------------------------------------------------------------- catalog
-- The restaurant aggregate. Hot query fields are columns; the public detail,
-- menu and floor plan are JSON documents read and written as a whole.
CREATE TABLE restaurants (
    id         TEXT PRIMARY KEY,
    seq        BIGINT NOT NULL DEFAULT 0,  -- insertion order (stable listing order)
    slug       TEXT NOT NULL UNIQUE,
    city_id    TEXT NOT NULL,
    status     TEXT NOT NULL CHECK (status IN ('ONBOARDING', 'PENDING_APPROVAL', 'LIVE', 'PAUSED', 'SUSPENDED')),
    country    TEXT NOT NULL DEFAULT '',
    short_code TEXT NOT NULL UNIQUE,
    pin        TEXT NOT NULL,
    breakfast  BIGINT NOT NULL DEFAULT 0,
    detail     TEXT NOT NULL,               -- JSON RestaurantDetail
    menu       TEXT NOT NULL,               -- JSON MenuCategory[]
    tables     TEXT NOT NULL,               -- JSON string[] (table numbers)
    updated_at TEXT NOT NULL
);
CREATE INDEX restaurants_city_status ON restaurants (city_id, status);

-- QR code -> table lookup (codes are derived from short_code + table number).
CREATE TABLE restaurant_tables (
    code          TEXT PRIMARY KEY,
    restaurant_id TEXT NOT NULL REFERENCES restaurants (id) ON DELETE CASCADE,
    table_number  TEXT NOT NULL
);
CREATE INDEX restaurant_tables_restaurant ON restaurant_tables (restaurant_id);

-- ---------------------------------------------------------------- bookings
CREATE TABLE bookings (
    id              TEXT PRIMARY KEY,
    user_id         TEXT NOT NULL,
    restaurant_id   TEXT NOT NULL,
    code            TEXT NOT NULL,
    date            TEXT NOT NULL,
    time            TEXT NOT NULL,
    guests          BIGINT NOT NULL,
    status          TEXT NOT NULL,
    cover_charge    BIGINT NOT NULL DEFAULT 0,
    idempotency_key TEXT NOT NULL,
    data            TEXT NOT NULL,          -- JSON Booking
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE (user_id, idempotency_key)
);
CREATE INDEX bookings_user ON bookings (user_id, date);
CREATE INDEX bookings_restaurant_slot ON bookings (restaurant_id, date, time, status);
CREATE INDEX bookings_created ON bookings (created_at);

-- ---------------------------------------------------------------- dine-in
CREATE TABLE table_sessions (
    id                TEXT PRIMARY KEY,
    restaurant_id     TEXT NOT NULL,
    table_code        TEXT NOT NULL,
    table_number      TEXT NOT NULL,
    status            TEXT NOT NULL CHECK (status IN ('ACTIVE', 'BILL_REQUESTED', 'CLOSED')),
    started_at        TEXT NOT NULL,
    bill_requested_at TEXT,
    bill_id           TEXT,
    closed_at         TEXT
);
CREATE INDEX table_sessions_table ON table_sessions (table_code, status);
CREATE INDEX table_sessions_restaurant ON table_sessions (restaurant_id, status);

CREATE TABLE orders (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL REFERENCES table_sessions (id),
    restaurant_id TEXT NOT NULL,
    status        TEXT NOT NULL,
    placed_at     TEXT NOT NULL,
    data          TEXT NOT NULL             -- JSON DineInOrder
);
CREATE INDEX orders_session ON orders (session_id);

CREATE TABLE service_requests (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL,
    restaurant_id TEXT NOT NULL,
    table_number  TEXT NOT NULL,
    action        TEXT NOT NULL,
    at            TEXT NOT NULL,
    resolved      BIGINT NOT NULL DEFAULT 0
);
CREATE INDEX service_requests_open ON service_requests (restaurant_id, resolved);

-- ---------------------------------------------------------------- billing
CREATE TABLE bills (
    id            TEXT PRIMARY KEY,
    restaurant_id TEXT NOT NULL,
    session_id    TEXT,
    data          TEXT NOT NULL,            -- JSON Bill
    created_at    TEXT NOT NULL
);

CREATE TABLE payment_intents (
    id           TEXT PRIMARY KEY,
    purpose      TEXT NOT NULL,
    reference_id TEXT NOT NULL,
    amount       BIGINT NOT NULL,
    status       TEXT NOT NULL,
    data         TEXT NOT NULL,             -- JSON PaymentIntent
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX payment_intents_reference ON payment_intents (reference_id);

CREATE TABLE bill_payments (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL,
    restaurant_id TEXT NOT NULL,
    bill_id       TEXT NOT NULL UNIQUE,     -- a bill can be paid once
    intent_id     TEXT NOT NULL,
    amount        BIGINT NOT NULL,
    paid_at       TEXT NOT NULL,
    data          TEXT NOT NULL             -- JSON BillPayment
);
CREATE INDEX bill_payments_user ON bill_payments (user_id, paid_at);
CREATE INDEX bill_payments_restaurant ON bill_payments (restaurant_id, paid_at);

-- ---------------------------------------------------------------- transactional outbox
-- Events are inserted in the same transaction as the state change, then relayed
-- to the message bus by the worker. published_at NULL = pending.
CREATE TABLE outbox (
    seq          BIGSERIAL PRIMARY KEY,
    event_id     TEXT NOT NULL UNIQUE,
    event_type   TEXT NOT NULL,
    payload      TEXT NOT NULL,             -- JSON EventEnvelope
    created_at   TEXT NOT NULL,
    published_at TEXT,
    attempts     BIGINT NOT NULL DEFAULT 0,
    last_error   TEXT
);
CREATE INDEX outbox_pending ON outbox (published_at, seq);
