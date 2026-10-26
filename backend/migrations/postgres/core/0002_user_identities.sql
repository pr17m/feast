-- PostgreSQL version of migrations/sqlite/core/0002_user_identities.sql (keep both in sync).
-- Social login identities (Google, Apple, Facebook…) linked to users.
-- A user can have many; each (provider, subject) belongs to exactly one user.
CREATE TABLE user_identities (
    provider    TEXT NOT NULL,               -- GOOGLE | APPLE | FACEBOOK
    subject     TEXT NOT NULL,               -- provider's stable user id
    user_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    email       TEXT,                        -- as asserted by the provider (informational)
    created_at  TEXT NOT NULL,
    PRIMARY KEY (provider, subject)
);

CREATE INDEX idx_user_identities_user ON user_identities (user_id);
