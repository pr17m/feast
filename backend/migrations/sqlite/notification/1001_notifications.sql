-- Notification service schema (its own database; versions start at 1001 so
-- they never collide with core migrations if both share one database in dev).
CREATE TABLE notifications (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL,
    restaurant_id TEXT,
    kind          TEXT NOT NULL,
    title         TEXT NOT NULL,
    body          TEXT NOT NULL,
    link          TEXT,
    event_id      TEXT NOT NULL,
    event_type    TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    read_at       TEXT,
    -- At-least-once event delivery must never notify twice.
    UNIQUE (event_id, user_id)
);

CREATE INDEX idx_notifications_user ON notifications (user_id, created_at DESC);
CREATE INDEX idx_notifications_unread ON notifications (user_id) WHERE read_at IS NULL;
