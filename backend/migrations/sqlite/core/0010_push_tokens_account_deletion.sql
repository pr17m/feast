-- Phone apps (Feast, Feast Partner): push tokens and each user's push switch;
-- account deletion (App Store / Google Play): users.deleted_at marks a deleted
-- account (stored with status BLOCKED, personal data cleared).
-- Keep in sync with the other dialect's 0010.
ALTER TABLE users ADD COLUMN deleted_at TEXT;
ALTER TABLE users ADD COLUMN push_enabled INTEGER NOT NULL DEFAULT 1;

CREATE TABLE push_tokens (
    token         TEXT PRIMARY KEY,         -- FCM registration token
    user_id       TEXT NOT NULL,
    platform      TEXT NOT NULL,            -- ios | android
    app           TEXT NOT NULL,            -- diner | partner
    created_at    TEXT NOT NULL,
    last_seen_at  TEXT NOT NULL
);
CREATE INDEX push_tokens_user ON push_tokens (user_id);
