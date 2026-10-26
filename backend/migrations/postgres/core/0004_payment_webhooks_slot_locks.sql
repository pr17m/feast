-- PostgreSQL version of migrations/sqlite/core/0004_payment_webhooks_slot_locks.sql (keep both in sync).
-- Payment webhooks: find an intent by the provider's order/payment-intent id.
ALTER TABLE payment_intents ADD COLUMN provider TEXT;
ALTER TABLE payment_intents ADD COLUMN provider_ref TEXT;
CREATE INDEX payment_intents_provider_ref ON payment_intents (provider, provider_ref);

-- One row per bookable slot. Booking takes a write lock on its slot's row
-- before counting capacity, so concurrent bookings (on any number of API
-- instances) are serialised per slot and the last table is sold once.
CREATE TABLE slot_locks (
    restaurant_id TEXT NOT NULL,
    date          TEXT NOT NULL,
    time          TEXT NOT NULL,
    version       BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (restaurant_id, date, time)
);
