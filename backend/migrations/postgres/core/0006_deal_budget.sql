-- PostgreSQL version.
-- Daily deal budget: one claim per discounted bill. A claim is HELD while the
-- payment is in flight (it stops counting after the payment hold window),
-- USED once the payment succeeds, RELEASED if it fails. Keep in sync with
-- migrations/sqlite/core/0006_deal_budget.sql.
CREATE TABLE deal_claims (
    bill_id       TEXT PRIMARY KEY,
    restaurant_id TEXT NOT NULL,
    date          TEXT NOT NULL,
    intent_id     TEXT NOT NULL,
    status        TEXT NOT NULL,
    claimed_at    TEXT NOT NULL
);
CREATE INDEX deal_claims_day ON deal_claims (restaurant_id, date, status);

-- One row per restaurant per local day. Claiming a deal takes a write lock on
-- it before counting, so concurrent payments (on any number of API instances)
-- can't exceed the restaurant's daily limit.
CREATE TABLE deal_budget_days (
    restaurant_id TEXT NOT NULL,
    date          TEXT NOT NULL,
    version       BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (restaurant_id, date)
);
