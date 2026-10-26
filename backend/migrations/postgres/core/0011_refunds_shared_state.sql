-- Refunds sent through the payment provider, and state every API instance and
-- worker must share instead of process memory: rate-limit counters, one-time
-- code challenges, leases (one outbox relay / sweeper at a time) and the
-- record of pushed notifications (each pushed once).
-- Keep in sync with the other dialect's 0011.
CREATE TABLE refunds (
    id                 TEXT PRIMARY KEY,
    intent_id          TEXT NOT NULL,
    provider           TEXT NOT NULL,
    provider_refund_id TEXT,
    status             TEXT NOT NULL,          -- PENDING | SUCCEEDED | FAILED
    idempotency_key    TEXT NOT NULL UNIQUE,   -- one refund per reason
    amount             BIGINT NOT NULL,
    data               TEXT NOT NULL,          -- JSON Refund
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL
);
CREATE INDEX refunds_intent ON refunds (intent_id);
CREATE INDEX refunds_provider ON refunds (provider, provider_refund_id);
CREATE INDEX refunds_status ON refunds (status, created_at);

CREATE TABLE rate_limit_hits (
    key         TEXT NOT NULL,                 -- route|client ip
    win         BIGINT NOT NULL,                   -- window index (epoch minutes)
    hits        BIGINT NOT NULL,
    expires_ms  BIGINT NOT NULL,
    PRIMARY KEY (key, win)
);
CREATE INDEX rate_limit_hits_expiry ON rate_limit_hits (expires_ms);

CREATE TABLE otp_challenges (
    subject     TEXT PRIMARY KEY,              -- phone number
    code_hash   TEXT NOT NULL,                 -- sha256 of subject|code
    attempts    BIGINT NOT NULL DEFAULT 0,
    expires_ms  BIGINT NOT NULL
);

CREATE TABLE leases (
    name        TEXT PRIMARY KEY,              -- outbox-relay, sweeper
    holder      TEXT NOT NULL,
    until_ms    BIGINT NOT NULL
);

-- One row per (notification, phone): claimed by one worker, so a push that
-- succeeded is never sent again; transient failures are retried with backoff.
CREATE TABLE push_deliveries (
    notification_id TEXT NOT NULL,
    token           TEXT NOT NULL,
    status          TEXT NOT NULL,            -- SENDING | SENT | RETRY | FAILED | GAVE_UP | DROPPED
    attempts        BIGINT NOT NULL DEFAULT 1,
    next_at_ms      BIGINT NOT NULL DEFAULT 0,  -- RETRY: when to try again
    lease_until_ms  BIGINT NOT NULL DEFAULT 0,  -- SENDING: the claim expires (worker died)
    error           TEXT,
    data            TEXT NOT NULL,            -- the notification (for retries)
    claimed_at      TEXT NOT NULL,
    PRIMARY KEY (notification_id, token)
);
CREATE INDEX push_deliveries_due ON push_deliveries (status, next_at_ms);
