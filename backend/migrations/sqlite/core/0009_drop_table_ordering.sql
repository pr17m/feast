-- Ordering from the table, table sessions, staff requests and the table PIN
-- were removed from Feast. A restaurant keeps one printed QR (its short_code,
-- /r/{code}) that opens its page to book or pay the bill for a Feast booking.
-- Keep in sync with the other dialect's 0009.
DROP TABLE service_requests;
DROP TABLE orders;
DROP TABLE table_sessions;
DROP TABLE restaurant_tables;

ALTER TABLE restaurants DROP COLUMN pin;
ALTER TABLE restaurants DROP COLUMN tables;
ALTER TABLE bills DROP COLUMN session_id;
