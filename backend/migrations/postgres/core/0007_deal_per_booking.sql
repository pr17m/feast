-- One Feast deal per booking: deals are only for diners who booked on Feast,
-- and each booking gets one discounted bill. claim_deal refuses a second
-- bill for the same booking while another one holds or used its deal.
-- Keep in sync with migrations/sqlite/core/0007_deal_per_booking.sql.
ALTER TABLE deal_claims ADD COLUMN booking_id TEXT;
CREATE INDEX deal_claims_booking ON deal_claims (booking_id);
