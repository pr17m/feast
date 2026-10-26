-- Markets (countries), cities, platform subscriptions and onboarding documents.
-- A deployment serves one country, but the schema allows several (tests, staging).

-- ---------------------------------------------------------------- markets
-- Seeded from config/countries/<cc>.yaml on first start; admins edit afterwards.
CREATE TABLE markets (
    country_code TEXT PRIMARY KEY,          -- ISO 3166-1 alpha-2, upper case
    status       TEXT NOT NULL CHECK (status IN ('LIVE', 'COMING_SOON')),
    data         TEXT NOT NULL,             -- JSON Market (never contains secrets)
    updated_at   TEXT NOT NULL
);

CREATE TABLE cities (
    id           TEXT PRIMARY KEY,
    seq          INTEGER NOT NULL DEFAULT 0,
    country_code TEXT NOT NULL,
    name         TEXT NOT NULL,
    data         TEXT NOT NULL,             -- JSON City
    updated_at   TEXT NOT NULL
);
CREATE INDEX cities_country ON cities (country_code);

-- ---------------------------------------------------------------- subscriptions
-- One row per restaurant; status is derived from the dates (see domain::subscription).
CREATE TABLE subscriptions (
    restaurant_id TEXT PRIMARY KEY,
    trial_ends_at TEXT,
    paid_through  TEXT,
    plan_id       TEXT,
    updated_at    TEXT NOT NULL
);

-- Payments restaurants make to the platform (and admin-granted COMP periods).
CREATE TABLE platform_payments (
    id             TEXT PRIMARY KEY,
    invoice_number TEXT NOT NULL UNIQUE,
    restaurant_id  TEXT NOT NULL,
    status         TEXT NOT NULL CHECK (status IN ('PAID', 'FAILED')),
    currency       TEXT NOT NULL,
    total          INTEGER NOT NULL,
    created_at     TEXT NOT NULL,
    data           TEXT NOT NULL            -- JSON PlatformPayment
);
CREATE INDEX platform_payments_restaurant ON platform_payments (restaurant_id, created_at);
CREATE INDEX platform_payments_created ON platform_payments (created_at);

-- Monotonic invoice numbering (gap-free per deployment).
CREATE TABLE invoice_seq (
    id    INTEGER PRIMARY KEY CHECK (id = 1),
    value INTEGER NOT NULL
);
INSERT INTO invoice_seq (id, value) VALUES (1, 0);

-- ---------------------------------------------------------------- onboarding & compliance
CREATE TABLE compliance_documents (
    id             TEXT PRIMARY KEY,
    restaurant_id  TEXT NOT NULL,
    requirement_id TEXT NOT NULL,
    status         TEXT NOT NULL CHECK (status IN ('SUBMITTED', 'APPROVED', 'REJECTED')),
    storage_key    TEXT NOT NULL,           -- FileStorage key (not exposed by the API)
    expires_on     TEXT,
    uploaded_at    TEXT NOT NULL,
    data           TEXT NOT NULL            -- JSON ComplianceDocument
);
CREATE INDEX compliance_documents_restaurant ON compliance_documents (restaurant_id, requirement_id, uploaded_at);

-- Admin review state of a restaurant's application.
CREATE TABLE restaurant_reviews (
    restaurant_id TEXT PRIMARY KEY,
    notes         TEXT,
    submitted_at  TEXT,
    updated_at    TEXT NOT NULL
);
