//! Table booking. Creating a booking writes the booking and a `booking.created`
//! event atomically; the worker turns that event into notifications for the
//! restaurant's owners.

use feast_domain::model::*;
use feast_domain::pricing::cover_charge_for;
use feast_domain::slots::{compute_slots, feast_tables, SlotContext, PAYMENT_HOLD_MINUTES};

use crate::ports::SlotRoom;
use feast_domain::{DomainEvent, Party};

use crate::context::{valid_phone, Feast};
use crate::error::{AppError, AppResult};

impl Feast {
    /// Bookings auto-complete 3 hours after their slot (`tz`: the restaurant's city).
    pub(crate) fn effective_booking(&self, mut b: Booking, tz: &str) -> Booking {
        if matches!(b.status, BookingStatus::Confirmed | BookingStatus::Seated) {
            if let Some(at) = self.p.clock.local_to_ms(tz, &b.date, &b.time) {
                if at + 3 * 3_600_000 < self.p.clock.now_ms() {
                    b.status = BookingStatus::Completed;
                }
            }
        }
        b
    }

    /// Unpaid bookings created after this instant still hold their table.
    pub(crate) fn hold_cutoff(&self) -> String {
        feast_domain::subscription::iso(self.now() - chrono::Duration::minutes(PAYMENT_HOLD_MINUTES))
    }

    pub(crate) async fn slots_for(&self, r: &Restaurant, date: &str, guests: i64) -> AppResult<Vec<Slot>> {
        if chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_err() {
            return Err(AppError::validation("INVALID_DATE", "Date must be YYYY-MM-DD"));
        }
        let held = self.p.bookings.tables_held(&r.detail.id, date, &self.hold_cutoff()).await?;
        let tz = self.city_tz(&r.detail.city_id).await?;
        let today = self.p.clock.local_date(&tz, 0);
        let ctx = SlotContext { date, guests, today: &today, now_minutes: self.p.clock.local_minutes(&tz) };
        Ok(compute_slots(r, &ctx, |time| held.get(time).copied().unwrap_or(0)))
    }

    pub async fn slots(&self, restaurant_id: &str, date: &str, guests: i64) -> AppResult<SlotAvailability> {
        let r = self.public_restaurant(restaurant_id).await?;
        let guests = guests.clamp(1, 20);
        let slots = self.slots_for(&r, date, guests).await?;
        Ok(SlotAvailability { restaurant_id: r.detail.id, date: date.into(), guests, slots })
    }

    pub async fn create_booking(&self, user: &User, req: CreateBookingRequest) -> AppResult<CreateBookingResponse> {
        // Idempotency: a retried request returns the original booking.
        if req.idempotency_key.trim().is_empty() || req.idempotency_key.len() > 100 {
            return Err(AppError::validation("INVALID_IDEMPOTENCY_KEY", "idempotencyKey is required"));
        }
        if let Some(existing) = self.p.bookings.find_by_idempotency_key(&user.id, &req.idempotency_key).await? {
            return Ok(CreateBookingResponse { booking: existing, payment_intent: None });
        }
        let phone = req.contact.phone.trim().to_owned();
        let markets = self.p.markets.list_markets().await?;
        if req.contact.name.trim().is_empty() || !valid_phone(&markets, &phone) {
            return Err(AppError::validation("INVALID_CONTACT", "Enter your name and a valid mobile number"));
        }
        if !(1..=20).contains(&req.guests) {
            return Err(AppError::validation("INVALID_GUESTS", "Bookings are for 1–20 guests"));
        }
        let r = self.public_restaurant(&req.restaurant_id).await?;
        if !r.detail.is_open_for(Service::Bookings) {
            return Err(AppError::conflict("NOT_ACCEPTING", format!("{} isn't accepting table bookings on Feast right now", r.detail.name)));
        }
        let slot = self
            .slots_for(&r, &req.date, req.guests)
            .await?
            .into_iter()
            .find(|s| s.id == req.slot_id && s.available)
            .ok_or_else(|| AppError::conflict("SLOT_UNAVAILABLE", "That slot just filled up. Please pick another time."))?;

        let (market, _) = self.pricing_for(&r.detail).await?;
        let cover = cover_charge_for(r.detail.price_band, req.guests, &market);
        let booking = Booking {
            id: self.p.ids.new_id("bk"),
            user_id: user.id.clone(),
            code: format!("FST-{}", self.p.ids.short_code(4)),
            restaurant: r.detail.reference(),
            date: req.date.clone(),
            time: slot.time.clone(),
            guests: req.guests,
            deal: slot.deal.clone(),
            special_request: req.special_request.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| s.chars().take(200).collect()),
            contact: Contact { name: req.contact.name.trim().to_owned(), phone },
            status: if cover > 0 { BookingStatus::PendingPayment } else { BookingStatus::Confirmed },
            cover_charge: cover,
            currency: market.currency.clone(),
            payment: None,
            created_at: self.p.clock.now_iso(),
            checked_in_at: None,
        };

        // Only confirmed bookings notify the restaurant; paid ones notify on payment.
        let events = if booking.status == BookingStatus::Confirmed {
            vec![self.event(Some(&r.detail.id), Some(&user.id), DomainEvent::BookingCreated { booking: booking.clone() })]
        } else {
            vec![]
        };
        // The slot list above is advisory; this is the real check. It runs under a
        // per-slot lock in the database, so two diners racing for the last table
        // (on the same or different API instances) can't both get it.
        let room = SlotRoom { tables: feast_tables(&r, &booking.date, &booking.time), pending_since: self.hold_cutoff() };
        if !self.p.bookings.insert_if_room(&booking, &req.idempotency_key, &events, &room).await? {
            return Err(AppError::conflict("SLOT_UNAVAILABLE", "That slot just filled up. Please pick another time."));
        }

        let payment_intent = if cover > 0 { Some(self.new_intent(user, &market, PaymentPurpose::Booking, &booking.id, cover, None).await?) } else { None };
        Ok(CreateBookingResponse { booking, payment_intent })
    }

    /// The booking's diner, an owner of its restaurant, or an admin.
    async fn visible_booking(&self, user: &User, id: &str) -> AppResult<Booking> {
        let b = self.p.bookings.get(id).await?.ok_or_else(|| AppError::not_found("Booking"))?;
        if b.user_id == user.id || user.can_manage(&b.restaurant.id) {
            Ok(b)
        } else {
            Err(AppError::not_found("Booking"))
        }
    }

    pub(crate) async fn restaurant_tz(&self, restaurant_id: &str) -> AppResult<String> {
        match self.p.restaurants.get(restaurant_id).await? {
            Some(r) => self.city_tz(&r.detail.city_id).await,
            None => Ok(String::new()),
        }
    }

    pub async fn booking(&self, user: &User, id: &str) -> AppResult<Booking> {
        let b = self.visible_booking(user, id).await?;
        let tz = self.restaurant_tz(&b.restaurant.id).await?;
        Ok(self.effective_booking(b, &tz))
    }

    pub async fn my_bookings(&self, user: &User) -> AppResult<Vec<Booking>> {
        let tz = self.tz_by_restaurant().await?;
        let mut list: Vec<Booking> = self
            .p
            .bookings
            .list_by_user(&user.id)
            .await?
            .into_iter()
            .filter(|b| b.status != BookingStatus::PendingPayment)
            .map(|b| {
                let z = tz.get(&b.restaurant.id).cloned().unwrap_or_default();
                self.effective_booking(b, &z)
            })
            .collect();
        list.sort_by(|a, b| (&a.date, &a.time).cmp(&(&b.date, &b.time)));
        Ok(list)
    }

    /// "I'm here": the diner tells the restaurant they've arrived. Allowed from
    /// 30 minutes before the slot to 2 hours after; repeating it changes nothing.
    pub async fn check_in_booking(&self, user: &User, id: &str) -> AppResult<Booking> {
        let b = self.p.bookings.get(id).await?.filter(|b| b.user_id == user.id).ok_or_else(|| AppError::not_found("Booking"))?;
        let tz = self.restaurant_tz(&b.restaurant.id).await?;
        let b = self.effective_booking(b, &tz);
        let at = self.p.clock.local_to_ms(&tz, &b.date, &b.time).ok_or_else(|| AppError::conflict("NOT_CHECKABLE", "This booking's time is invalid"))?;
        if let Some(why) = feast_domain::arrival::check_in_blocker(b.status, at, self.p.clock.now_ms()) {
            return Err(AppError::conflict("CHECK_IN_UNAVAILABLE", why));
        }
        if b.checked_in_at.is_some() {
            return Ok(b);
        }
        let mut updated = b;
        updated.checked_in_at = Some(self.p.clock.now_iso());
        let ev = self.event(Some(&updated.restaurant.id), Some(&updated.user_id), DomainEvent::BookingCheckedIn { booking: updated.clone() });
        self.p.bookings.update(&updated, &[ev]).await?;
        Ok(updated)
    }

    pub async fn cancel_booking(&self, user: &User, id: &str) -> AppResult<Booking> {
        let b = self.visible_booking(user, id).await?;
        let tz = self.restaurant_tz(&b.restaurant.id).await?;
        if self.effective_booking(b.clone(), &tz).status != BookingStatus::Confirmed {
            return Err(AppError::conflict("NOT_CANCELLABLE", "This booking can no longer be cancelled"));
        }
        let by = if b.user_id == user.id {
            Party::Customer
        } else if user.role == Role::Admin {
            Party::Admin
        } else {
            Party::Restaurant
        };
        let mut updated = b;
        updated.status = BookingStatus::Cancelled;
        let ev = self.event(Some(&updated.restaurant.id), Some(&updated.user_id), DomainEvent::BookingCancelled { booking: updated.clone(), by });
        // Only from CONFIRMED, and only once: of two cancellations (or a
        // cancellation racing a check-in) one wins; the other changes nothing,
        // so there is one event, one notification, one audit record and one refund.
        if !self.p.bookings.update_if_status(&updated, BookingStatus::Confirmed, &[ev]).await? {
            return Err(AppError::conflict("NOT_CANCELLABLE", "This booking was just changed and can no longer be cancelled"));
        }
        // An upfront table charge goes back per the refunds policy (in time, or
        // cancelled by the restaurant/Feast). A refund that can't be sent right
        // now is retried by the sweeper; it never undoes the cancellation.
        if let Err(e) = self.refund_cancelled_booking(&updated, by, false).await {
            tracing::error!(booking = %updated.id, error = %e.message, "couldn't start the refund for a cancelled booking");
        }
        Ok(updated)
    }
}
