//! Partner (restaurant owner) portal. Reads need the restaurant's OWNER or an
//! ADMIN; changes need the OWNER (Feast staff only with owner-granted support
//! access, and every staff change is audited — see `trust.rs`).

use feast_domain::model::*;
use feast_domain::{DomainEvent, Party};

use crate::context::Feast;
use crate::error::{AppError, AppResult};

impl Feast {
    pub async fn partner_restaurants(&self, user: &User) -> AppResult<Vec<RestaurantDetail>> {
        self.require_role(user, &[Role::Owner, Role::Admin])?;
        Ok(self.p.restaurants.list().await?.into_iter().filter(|r| user.can_manage(&r.detail.id)).map(|r| r.detail.with_derived_flags()).collect())
    }

    pub async fn partner_dashboard(&self, user: &User, restaurant_id: &str) -> AppResult<PartnerDashboard> {
        let r = self.managed_restaurant(user, restaurant_id).await?;
        let tz = self.city_tz(&r.detail.city_id).await?;
        let today = self.p.clock.local_date(&tz, 0);
        let week_start = self.p.clock.local_date(&tz, -6);
        let bookings: Vec<Booking> = self
            .p
            .bookings
            .list_by_restaurant_between(restaurant_id, &week_start, &today)
            .await?
            .into_iter()
            .filter(|b| !matches!(b.status, BookingStatus::Cancelled | BookingStatus::PendingPayment))
            .map(|b| self.effective_booking(b, &tz))
            .collect();
        let todays: Vec<&Booking> = bookings.iter().filter(|b| b.date == today).collect();
        let pays = self.p.billing.payments_by_restaurant(restaurant_id).await?;
        let pay_date = |p: &BillPayment| self.p.clock.local_date_of(&tz, &p.payment.paid_at).unwrap_or_default();
        let pending_counter_payments = self.p.offline.offline_for_restaurant(restaurant_id, Some(OfflinePaymentStatus::PendingApproval), 500).await?.len();
        let last7_days = (-6..=0)
            .map(|d| {
                let date = self.p.clock.local_date(&tz, d);
                DayStat {
                    bookings: bookings.iter().filter(|b| b.date == date).count(),
                    revenue: pays.iter().filter(|p| pay_date(p) == date).map(|p| p.payment.amount).sum(),
                    date,
                }
            })
            .collect();
        let paid_today: Vec<&BillPayment> = pays.iter().filter(|p| pay_date(p) == today).collect();
        Ok(PartnerDashboard {
            restaurant: r.detail.clone().with_derived_flags(),
            date: today,
            bookings_today: todays.len(),
            covers_today: todays.iter().map(|b| b.guests).sum(),
            seated_now: todays.iter().filter(|b| b.status == BookingStatus::Seated).count(),
            arrived_today: todays.iter().filter(|b| b.checked_in_at.is_some()).count(),
            pending_counter_payments,
            revenue_today: paid_today.iter().map(|p| p.payment.amount).sum(),
            savings_given_today: paid_today.iter().map(|p| p.bill.savings).sum(),
            last7_days,
        })
    }

    pub async fn partner_bookings(&self, user: &User, restaurant_id: &str, date: &str) -> AppResult<Vec<Booking>> {
        let r = self.managed_restaurant(user, restaurant_id).await?;
        let tz = self.city_tz(&r.detail.city_id).await?;
        let mut list: Vec<Booking> = self
            .p
            .bookings
            .list_by_restaurant_between(restaurant_id, date, date)
            .await?
            .into_iter()
            .filter(|b| b.status != BookingStatus::PendingPayment)
            .map(|b| self.effective_booking(b, &tz))
            .collect();
        list.sort_by(|a, b| a.time.cmp(&b.time));
        Ok(list)
    }

    pub async fn update_booking_status(&self, user: &User, booking_id: &str, status: BookingStatus) -> AppResult<Booking> {
        let b = self.p.bookings.get(booking_id).await?.ok_or_else(|| AppError::not_found("Booking"))?;
        if user.role == Role::Admin && status == BookingStatus::Cancelled {
            // Feast may cancel a booking to resolve a dispute; it's audited.
            self.managed_restaurant(user, &b.restaurant.id).await?;
        } else {
            self.editable_restaurant(user, &b.restaurant.id).await?;
        }
        let allowed: &[BookingStatus] = match b.status {
            BookingStatus::Confirmed => &[BookingStatus::Seated, BookingStatus::NoShow, BookingStatus::Cancelled],
            BookingStatus::Seated => &[BookingStatus::Completed],
            _ => &[],
        };
        if !allowed.contains(&status) {
            return Err(AppError::conflict(
                "INVALID_TRANSITION",
                format!("Can't move a {} booking to {}", b.status.as_str().to_lowercase(), status.as_str().to_lowercase()),
            ));
        }
        let from = b.status;
        let mut updated = b;
        updated.status = status;
        let ev = if status == BookingStatus::Cancelled {
            DomainEvent::BookingCancelled { booking: updated.clone(), by: if user.role == Role::Admin { Party::Admin } else { Party::Restaurant } }
        } else {
            DomainEvent::BookingStatusChanged { booking: updated.clone(), from }
        };
        let ev = self.event(Some(&updated.restaurant.id), Some(&updated.user_id), ev);
        self.p.bookings.update(&updated, &[ev]).await?;
        self.audit_staff_edit(user, &updated.restaurant.id, "booking.status_changed", format!("Booking {} → {}", updated.code, status.as_str().to_lowercase()))
            .await?;
        Ok(updated)
    }

    /// The restaurant's printed QR: `/r/{code}` opens its page, where a diner
    /// with a Feast booking today pays the bill and gets the deal.
    pub async fn partner_qr(&self, user: &User, restaurant_id: &str) -> AppResult<RestaurantQr> {
        let r = self.managed_restaurant(user, restaurant_id).await?;
        Ok(RestaurantQr { code: r.short_code.clone(), restaurant: r.detail.summary() })
    }

    pub async fn partner_payments(&self, user: &User, restaurant_id: &str) -> AppResult<Vec<BillPayment>> {
        self.managed_restaurant(user, restaurant_id).await?;
        let mut list = self.p.billing.payments_by_restaurant(restaurant_id).await?;
        list.sort_by(|a, b| b.payment.paid_at.cmp(&a.payment.paid_at));
        Ok(list)
    }

    pub async fn partner_menu(&self, user: &User, restaurant_id: &str) -> AppResult<Menu> {
        let r = self.managed_restaurant(user, restaurant_id).await?;
        Ok(Menu { restaurant_id: r.detail.id, categories: r.menu })
    }

    pub async fn update_menu_item(&self, user: &User, restaurant_id: &str, item_id: &str, patch: MenuItemPatch) -> AppResult<MenuItem> {
        let mut r = self.editable_restaurant(user, restaurant_id).await?;
        if let Some(p) = patch.price {
            if !(100..=10_000_000).contains(&p) {
                return Err(AppError::validation("INVALID_PRICE", "Enter a price between ₹1 and ₹1,00,000"));
            }
        }
        let item = r.find_item_mut(item_id).ok_or_else(|| AppError::not_found("Menu item"))?;
        if let Some(p) = patch.price {
            item.price = p;
        }
        if let Some(a) = patch.available {
            item.available = a;
        }
        let item = item.clone();
        self.p.restaurants.update(&r).await?;
        let mut changes = vec![];
        if let Some(p) = patch.price {
            changes.push(format!("price {}", crate::notifications::fmt_money(p, &r.detail.currency)));
        }
        if let Some(a) = patch.available {
            changes.push(if a { "available".to_owned() } else { "sold out".to_owned() });
        }
        self.audit_staff_edit(user, restaurant_id, "menu.item_updated", format!("{}: {}", item.name, changes.join(", "))).await?;
        Ok(item)
    }

    pub async fn update_offers(&self, user: &User, restaurant_id: &str, offers: Vec<Offer>) -> AppResult<Vec<Offer>> {
        let mut r = self.managed_restaurant(user, restaurant_id).await?;
        // Moderation is Feast's to set: carry it over by offer id, whatever the client sent.
        let moderation: std::collections::HashMap<String, Moderation> =
            r.detail.offers.iter().filter_map(|o| o.moderation.clone().map(|m| (o.id.clone(), m))).collect();
        let offers: Vec<Offer> = offers
            .into_iter()
            .map(|mut o| {
                o.moderation = moderation.get(&o.id).cloned();
                o
            })
            .collect();
        let bank = |list: &[Offer]| list.iter().filter(|o| o.scope == OfferScope::Bank).cloned().collect::<Vec<_>>();
        let own = |list: &[Offer]| list.iter().filter(|o| o.scope != OfferScope::Bank).cloned().collect::<Vec<_>>();
        if user.role == Role::Owner {
            // Bank offers are funded and managed by Feast.
            if bank(&offers) != bank(&r.detail.offers) {
                return Err(AppError::forbidden("Bank offers are managed by Feast"));
            }
        } else if own(&offers) != own(&r.detail.offers) && !self.staff_may_edit(&r).await? {
            // Feast manages bank offers; the restaurant's own offers are the owner's.
            return Err(AppError::new(
                crate::error::ErrorKind::Forbidden,
                "SUPPORT_ACCESS_REQUIRED",
                "Only the restaurant's owner can change its offers. Ask them to grant Feast support access in Settings.",
            ));
        }
        if offers.len() > 20 {
            return Err(AppError::validation("INVALID_OFFER", "Too many offers"));
        }
        for o in &offers {
            if o.title.trim().is_empty() {
                return Err(AppError::validation("INVALID_OFFER", "Every offer needs a title"));
            }
            if o.percent_off.is_some_and(|p| !(0..=60).contains(&p)) {
                return Err(AppError::validation("INVALID_OFFER", "Discounts must be between 0% and 60%"));
            }
            if o.max_discount.is_some_and(|m| m < 0) || o.flat_off.is_some_and(|f| f < 0) || o.min_bill.is_some_and(|m| m < 0) {
                return Err(AppError::validation("INVALID_OFFER", "Amounts can't be negative"));
            }
        }
        r.detail.offers = offers
            .into_iter()
            .map(|mut o| {
                if o.id.is_empty() || o.id.starts_with("new_") {
                    o.id = self.p.ids.new_id("off");
                }
                o
            })
            .collect();
        r.detail.headline_offer = r.detail.headline();
        self.p.restaurants.update(&r).await?;
        self.audit_staff_edit(user, restaurant_id, "offers.updated", format!("Saved {} offers", r.detail.offers.len())).await?;
        Ok(r.detail.offers)
    }

    pub async fn update_settings(&self, user: &User, restaurant_id: &str, patch: RestaurantSettingsPatch) -> AppResult<RestaurantSettings> {
        let mut r = self.editable_restaurant(user, restaurant_id).await?;
        if patch.tables_per_slot.is_some_and(|t| !(0..=100).contains(&t)) {
            return Err(AppError::validation("INVALID_SETTINGS", "Tables per slot must be between 0 and 100"));
        }
        if patch.max_discount_per_bill.is_some() || patch.daily_deal_limit.is_some() {
            let (market, _) = self.pricing_for(&r.detail).await?;
            let cap = patch.max_discount_per_bill.unwrap_or(r.detail.settings.max_discount_per_bill);
            let limit = patch.daily_deal_limit.unwrap_or(r.detail.settings.daily_deal_limit);
            feast_domain::deals::validate(cap, limit, &market).map_err(|m| AppError::validation("INVALID_SETTINGS", m))?;
        }
        let s = &mut r.detail.settings;
        if let Some(v) = patch.bookings_enabled {
            s.bookings_enabled = v;
        }
        if let Some(v) = patch.bill_payment_enabled {
            s.bill_payment_enabled = v;
        }
        if let Some(v) = patch.tables_per_slot {
            s.tables_per_slot = v;
        }
        if let Some(v) = patch.max_discount_per_bill {
            s.max_discount_per_bill = v;
        }
        if let Some(v) = patch.daily_deal_limit {
            s.daily_deal_limit = v;
        }
        let settings = s.clone();
        self.p.restaurants.update(&r).await?;
        self.audit_staff_edit(user, restaurant_id, "settings.updated", "Changed restaurant settings".into()).await?;
        Ok(settings)
    }

    /// Owner edits how the restaurant is listed and found (cuisines, experiences,
    /// cost for two → price band, pure veg, about). These drive customer filters.
    pub async fn update_listing(&self, user: &User, restaurant_id: &str, patch: ListingPatch) -> AppResult<RestaurantDetail> {
        use feast_domain::listing::*;
        let mut r = self.editable_restaurant(user, restaurant_id).await?;
        let others: Vec<RestaurantDetail> = self.p.restaurants.list().await?.into_iter().filter(|x| x.detail.id != r.detail.id).map(|x| x.detail).collect();
        let d = &mut r.detail;
        if let Some(c) = &patch.cuisines {
            let known: Vec<String> = others.iter().flat_map(|o| o.cuisines.iter().cloned()).collect();
            let c = clean_labels(c, &known, MAX_CUISINES);
            if c.is_empty() {
                return Err(AppError::validation("INVALID_LISTING", "Add at least one cuisine"));
            }
            d.cuisines = c;
        }
        if let Some(t) = &patch.tags {
            let known: Vec<String> = others.iter().flat_map(|o| o.tags.iter().cloned()).collect();
            d.tags = clean_labels(t, &known, MAX_TAGS);
        }
        if let Some(v) = patch.is_pure_veg {
            d.is_pure_veg = v;
        }
        if let Some(cost) = patch.cost_for_two {
            let m = self.market(&d.country_code).await?;
            let (min, max) = feast_domain::market::cost_limits(&m);
            if !(min..=max).contains(&cost) {
                let (a, b) = (feast_domain::market::money(min, &m), feast_domain::market::money(max, &m));
                return Err(AppError::validation("INVALID_LISTING", format!("Cost for two must be between {a} and {b}")));
            }
            d.cost_for_two = cost;
            d.price_band = feast_domain::market::price_band_for(cost, &m);
        }
        if let Some(a) = &patch.about {
            d.about = a.trim().chars().take(MAX_ABOUT_CHARS).collect();
        }
        if let Some(h) = &patch.highlights {
            d.highlights = clean_labels(h, &[], MAX_HIGHLIGHTS);
        }
        self.p.restaurants.update(&r).await?;
        self.audit_staff_edit(user, restaurant_id, "listing.updated", "Edited the listing".into()).await?;
        Ok(r.detail.with_derived_flags())
    }
}
