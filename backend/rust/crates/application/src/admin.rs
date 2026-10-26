//! Admin console: platform metrics, restaurant lifecycle (approval gated on
//! compliance), users and roles.

use std::collections::{BTreeMap, HashMap};

use feast_domain::market::{cost_limits, money, price_band_for};
use feast_domain::model::*;
use feast_domain::DomainEvent;

use crate::context::Feast;
use crate::error::{AppError, AppResult};

/// What's needed to create a restaurant (by an admin or a partner signing up).
pub(crate) struct NewRestaurant<'a> {
    pub name: &'a str,
    pub city_id: &'a str,
    pub area: &'a str,
    pub cuisines: &'a [String],
    pub cost_for_two: Paise,
    pub is_pure_veg: bool,
    pub tags: &'a [String],
    pub about: &'a str,
}

impl Feast {
    pub async fn admin_metrics(&self, user: &User) -> AppResult<AdminMetrics> {
        self.require_role(user, &[Role::Admin])?;
        let restaurants = self.p.restaurants.list().await?;
        let users = self.p.users.list().await?;
        let tz = self.tz_by_restaurant().await?;
        let bookings: Vec<Booking> = self
            .p
            .bookings
            .list_recent(100_000)
            .await?
            .into_iter()
            .filter(|b| b.status != BookingStatus::PendingPayment)
            .map(|b| {
                let z = tz.get(&b.restaurant.id).cloned().unwrap_or_default();
                self.effective_booking(b, &z)
            })
            .collect();
        let payments = self.p.billing.list_recent_payments(100_000).await?;

        let mut by_status = BTreeMap::new();
        for s in RestaurantStatus::ALL {
            by_status.insert(s.as_str().to_owned(), restaurants.iter().filter(|r| r.detail.status == s).count());
        }
        let mut by_role = BTreeMap::new();
        for r in Role::ALL {
            by_role.insert(r.as_str().to_owned(), users.iter().filter(|u| u.role == r).count());
        }
        // Money is never summed across currencies.
        let mut by_currency: BTreeMap<String, CurrencyTotals> = BTreeMap::new();
        for p in &payments {
            let e =
                by_currency.entry(p.payment.currency.clone()).or_insert_with(|| CurrencyTotals { currency: p.payment.currency.clone(), gmv: 0, savings: 0 });
            if p.payment.settlement == Settlement::Online {
                e.gmv += p.payment.amount; // GMV = through Feast (counter payments are settled offline)
            }
            e.savings += p.bill.savings;
        }
        let markets: HashMap<String, Market> = self.p.markets.list_markets().await?.into_iter().map(|m| (m.country_code.clone(), m)).collect();
        let by_city = self
            .p
            .markets
            .list_cities()
            .await?
            .into_iter()
            .map(|c| {
                let ids: Vec<&str> = restaurants.iter().filter(|r| r.detail.city_id == c.id).map(|r| r.detail.id.as_str()).collect();
                CityMetrics {
                    currency: markets.get(&c.country_code).map(|m| m.currency.clone()).unwrap_or_default(),
                    city_id: c.id,
                    city_name: c.name,
                    restaurants: ids.len(),
                    bookings: bookings.iter().filter(|b| ids.contains(&b.restaurant.id.as_str())).count(),
                    gmv: payments
                        .iter()
                        .filter(|p| p.payment.settlement == Settlement::Online && ids.contains(&p.restaurant.id.as_str()))
                        .map(|p| p.payment.amount)
                        .sum(),
                }
            })
            .collect();
        Ok(AdminMetrics {
            restaurants: by_status,
            users: by_role,
            bookings: BookingCounts {
                total: bookings.len(),
                upcoming: bookings.iter().filter(|b| b.status == BookingStatus::Confirmed).count(),
                cancelled: bookings.iter().filter(|b| b.status == BookingStatus::Cancelled).count(),
            },
            payments: PaymentTotals { count: payments.len(), by_currency: by_currency.into_values().collect() },
            subscriptions: self.subscription_metrics().await?,
            by_city,
        })
    }

    pub async fn admin_restaurants(&self, user: &User) -> AppResult<Vec<RestaurantDetail>> {
        self.require_role(user, &[Role::Admin])?;
        Ok(self.p.restaurants.list().await?.into_iter().map(|r| r.detail.with_derived_flags()).collect())
    }

    /// Validate and build a new restaurant in ONBOARDING (not visible to diners
    /// until its documents are approved and an admin approves it).
    pub(crate) async fn build_restaurant(&self, req: NewRestaurant<'_>) -> AppResult<Restaurant> {
        let name = req.name.trim();
        if name.chars().count() < 2 {
            return Err(AppError::validation("INVALID_NAME", "Enter the restaurant's name"));
        }
        let city = self.p.markets.get_city(req.city_id).await?.ok_or_else(|| AppError::validation("INVALID_CITY", "Pick a city"))?;
        let market = self.market(&city.country_code).await?;
        let cuisines: Vec<String> = req.cuisines.iter().map(|c| c.trim().to_owned()).filter(|c| !c.is_empty()).collect();
        if cuisines.is_empty() {
            return Err(AppError::validation("INVALID_CUISINES", "Add at least one cuisine"));
        }
        let (min, max) = cost_limits(&market);
        if !(min..=max).contains(&req.cost_for_two) {
            return Err(AppError::validation("INVALID_PRICE", format!("Cost for two must be between {} and {}", money(min, &market), money(max, &market))));
        }
        let base: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
        let mut base = base.split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
        if base.is_empty() {
            base = "restaurant".into();
        }
        let mut slug = base.clone();
        let mut n = 2;
        while self.p.restaurants.get_by_slug(&slug).await?.is_some() {
            slug = format!("{base}-{n}");
            n += 1;
        }
        let id = self.p.ids.new_id("r");
        let seed = feast_domain::slots::demand_hash(&slug);
        let area = if req.area.trim().is_empty() { city.areas.first().cloned().unwrap_or_default() } else { req.area.trim().to_owned() };
        let mut tags: Vec<String> = req.tags.iter().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()).collect();
        if req.is_pure_veg && !tags.iter().any(|t| t == "Pure veg") {
            tags.push("Pure veg".into());
        }
        let detail = RestaurantDetail {
            id,
            slug: slug.clone(),
            name: name.to_owned(),
            city_id: city.id.clone(),
            area: area.clone(),
            cuisines,
            rating: 0.0,
            rating_count: 0,
            cost_for_two: req.cost_for_two,
            price_band: price_band_for(req.cost_for_two, &market),
            currency: market.currency.clone(),
            country_code: market.country_code.clone(),
            distance_km: 1.0 + (seed % 90) as f64 / 10.0,
            is_pure_veg: req.is_pure_veg,
            tags,
            art: Art { hue: seed % 360, motif: CuisineMotif::ALL[(seed % 8) as usize], image: None, image_credit: None },
            headline_offer: None,
            accepts_bookings: false,
            accepts_bill_payment: false,
            status: RestaurantStatus::Onboarding,
            settings: RestaurantSettings::default(),
            address: format!("{area}, {}", city.name),
            about: req.about.trim().to_owned(),
            phone: String::new(),
            hours: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                .iter()
                .map(|d| OpeningHours { day: (*d).into(), open: "12:00".into(), close: "23:00".into() })
                .collect(),
            highlights: vec![],
            offers: vec![],
            photos: vec![],
            menu_preview: vec![],
        };
        let short: String = slug.chars().filter(|c| c.is_ascii_alphanumeric()).take(3).collect();
        Ok(Restaurant {
            detail,
            menu: vec![],
            breakfast: false,
            short_code: format!("{}{}", short.to_uppercase(), self.p.ids.short_code(2)),
            support_access_until: None,
            rating_base: None,
        })
    }

    pub async fn create_restaurant(&self, user: &User, req: CreateRestaurantRequest) -> AppResult<RestaurantDetail> {
        self.require_role(user, &[Role::Admin])?;
        let r = self
            .build_restaurant(NewRestaurant {
                name: &req.name,
                city_id: &req.city_id,
                area: &req.area,
                cuisines: &req.cuisines,
                cost_for_two: req.cost_for_two,
                is_pure_veg: req.is_pure_veg,
                tags: &req.tags,
                about: &req.about,
            })
            .await?;
        self.p.restaurants.insert(&r).await?;
        let id = r.detail.id.clone();
        self.audit_direct(Some(&id), "restaurant", &id, "restaurant.created", |a| {
            a.to_status = Some(status_tag(&r.detail.status));
            a.note = Some(r.detail.name.clone());
        })
        .await?;
        if let Some(owner_id) = &req.owner_user_id {
            if let Some(mut owner) = self.p.users.get(owner_id).await? {
                if owner.role == Role::Customer {
                    if owner.email.is_none() {
                        return Err(AppError::validation("INVALID_EMAIL", "The owner needs an email to sign in to the partner app"));
                    }
                    owner.role = Role::Owner;
                }
                if owner.role == Role::Owner && !owner.restaurant_ids.contains(&id) {
                    owner.restaurant_ids.push(id);
                }
                self.p.users.update(&owner).await?;
            }
        }
        Ok(r.detail.with_derived_flags())
    }

    pub async fn update_restaurant(&self, user: &User, restaurant_id: &str, patch: RestaurantPatch) -> AppResult<RestaurantDetail> {
        self.require_role(user, &[Role::Admin])?;
        let mut r = self.editable_restaurant(user, restaurant_id).await?;
        let d = &mut r.detail;
        if let Some(v) = patch.name {
            if v.trim().chars().count() < 2 {
                return Err(AppError::validation("INVALID_NAME", "Name is too short"));
            }
            d.name = v.trim().to_owned();
        }
        if let Some(v) = patch.area {
            d.area = v;
        }
        if let Some(v) = patch.cuisines {
            d.cuisines = v;
        }
        if let Some(v) = patch.cost_for_two {
            let m = self.market(&d.country_code).await?;
            let (min, max) = cost_limits(&m);
            if !(min..=max).contains(&v) {
                return Err(AppError::validation("INVALID_PRICE", format!("Cost for two must be between {} and {}", money(min, &m), money(max, &m))));
            }
            d.cost_for_two = v;
            d.price_band = price_band_for(v, &m);
        }
        if let Some(v) = patch.is_pure_veg {
            d.is_pure_veg = v;
        }
        if let Some(v) = patch.tags {
            d.tags = v;
        }
        if let Some(v) = patch.about {
            d.about = v;
        }
        if let Some(v) = patch.address {
            d.address = v;
        }
        if let Some(v) = patch.phone {
            let v = v.trim().to_owned();
            if !v.is_empty() && !self.is_valid_phone(&v).await? {
                return Err(AppError::validation("INVALID_PHONE", "Enter the phone number in international format, e.g. +61 2 9876 5432"));
            }
            d.phone = v;
        }
        self.p.restaurants.update(&r).await?;
        self.audit_staff_edit(user, restaurant_id, "restaurant.updated", "Edited restaurant details".into()).await?;
        Ok(r.detail.with_derived_flags())
    }

    /// Admins move restaurants through the lifecycle; going LIVE the first time
    /// requires every required document approved, and starts the free trial.
    /// Owners may only pause/resume their own live restaurant.
    pub async fn set_restaurant_status(&self, user: &User, restaurant_id: &str, status: RestaurantStatus) -> AppResult<RestaurantDetail> {
        self.require_role(user, &[Role::Owner, Role::Admin])?;
        let mut r = self.p.restaurants.get(restaurant_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
        let from = r.detail.status;
        if user.role == Role::Owner {
            if !user.can_manage(restaurant_id) {
                return Err(AppError::forbidden("You don't manage this restaurant"));
            }
            let ok = matches!((from, status), (RestaurantStatus::Live, RestaurantStatus::Paused) | (RestaurantStatus::Paused, RestaurantStatus::Live));
            if !ok {
                return Err(AppError::forbidden("Only Feast admins can approve or suspend restaurants"));
            }
        }
        if from == status {
            return Ok(r.detail.with_derived_flags());
        }
        let mut events = vec![];
        if status == RestaurantStatus::Live {
            let m = self.market(&r.detail.country_code).await?;
            let docs = self.p.compliance.documents_for(restaurant_id).await?;
            let tz = self.city_tz(&r.detail.city_id).await?;
            if !self.compliance_items(&m, &docs, &tz).1 {
                return Err(AppError::conflict(
                    "COMPLIANCE_INCOMPLETE",
                    "Approve every required document (and check none has expired) before this restaurant can go live",
                ));
            }
        }
        r.detail.status = status;
        self.p.restaurants.update(&r).await?;
        self.audit_direct(Some(restaurant_id), "restaurant", restaurant_id, "restaurant.status_changed", |a| {
            a.from_status = Some(status_tag(&from));
            a.to_status = Some(status_tag(&status));
        })
        .await?;
        if status == RestaurantStatus::Live && matches!(from, RestaurantStatus::Onboarding | RestaurantStatus::PendingApproval) {
            let existing = self.p.subscriptions.get_record(restaurant_id).await?;
            let rec = feast_domain::subscription::start_trial(existing.clone(), self.now(), self.settings.subscription_rules);
            if existing.as_ref() != Some(&rec) {
                self.p.subscriptions.put_record(restaurant_id, &rec).await?;
            }
            let mut info = self.p.compliance.review_info(restaurant_id).await?;
            info.notes = None;
            self.p.compliance.set_review_info(restaurant_id, &info).await?;
            events.push(self.event(
                Some(restaurant_id),
                Some(&user.id),
                DomainEvent::RestaurantApproved { restaurant: r.detail.reference(), trial_ends_at: rec.trial_ends_at.clone() },
            ));
        }
        if !events.is_empty() {
            self.p.outbox.append(&events).await?;
        }
        Ok(r.detail.with_derived_flags())
    }

    pub async fn admin_users(&self, user: &User) -> AppResult<Vec<User>> {
        self.require_role(user, &[Role::Admin])?;
        let mut list = self.p.users.list().await?;
        list.sort_by(|a, b| a.role.cmp(&b.role).then(a.name.cmp(&b.name)));
        Ok(list)
    }

    pub async fn create_user(&self, user: &User, req: CreateUserRequest) -> AppResult<User> {
        self.require_role(user, &[Role::Admin])?;
        let name = req.name.trim();
        if name.chars().count() < 2 {
            return Err(AppError::validation("INVALID_NAME", "Enter a name"));
        }
        let email = req.email.as_deref().map(str::trim).filter(|e| !e.is_empty());
        let phone = req.phone.as_deref().map(str::trim).filter(|p| !p.is_empty());
        if req.role != Role::Customer && !email.is_some_and(|e| e.contains('@')) {
            return Err(AppError::validation("INVALID_EMAIL", "Owners and admins need an email to sign in"));
        }
        if let Some(p) = phone {
            if !self.is_valid_phone(p).await? {
                return Err(AppError::validation("INVALID_PHONE", "Enter the phone number in international format, e.g. +91 98765 43210"));
            }
            if self.p.users.find_by_phone(p).await?.is_some() {
                return Err(AppError::conflict("DUPLICATE", "A user with this phone already exists"));
            }
        }
        if let Some(e) = email {
            if self.p.users.find_by_email(e).await?.is_some() {
                return Err(AppError::conflict("DUPLICATE", "A user with this email already exists"));
            }
        }
        let u = User {
            id: self.p.ids.new_id("u"),
            name: name.to_owned(),
            phone: phone.map(str::to_owned),
            email: email.map(str::to_owned),
            role: req.role,
            restaurant_ids: if req.role == Role::Owner { req.restaurant_ids.unwrap_or_default() } else { vec![] },
            status: UserStatus::Active,
            created_at: self.p.clock.now_iso(),
        };
        // New partner/admin accounts get the configured initial password (demo) — real
        // deployments should send an invite link instead.
        let hash = if u.email.is_some() { Some(self.p.passwords.hash(&self.initial_password())?) } else { None };
        self.p.users.insert(&u, hash.as_deref()).await?;
        self.audit_direct(None, "user", &u.id, "user.created", |a| {
            a.to_status = Some(status_tag(&u.status));
            a.note = Some(format!("{} · {}", u.name, u.role.as_str()));
            a.subject_user_id = Some(u.id.clone());
        })
        .await?;
        Ok(u)
    }

    pub async fn update_user(&self, user: &User, user_id: &str, patch: UserPatch) -> AppResult<User> {
        self.require_role(user, &[Role::Admin])?;
        let mut u = self.p.users.get(user_id).await?.ok_or_else(|| AppError::not_found("User"))?;
        let before = u.clone();
        if u.status == UserStatus::Deleted {
            return Err(AppError::conflict("ACCOUNT_DELETED", "This account was deleted by its owner and can't be changed"));
        }
        if patch.status == Some(UserStatus::Deleted) {
            return Err(AppError::validation("INVALID_STATUS", "Accounts are deleted by their owners from the app"));
        }
        if u.id == user.id && (patch.role.is_some_and(|r| r != Role::Admin) || patch.status == Some(UserStatus::Blocked)) {
            return Err(AppError::validation("SELF_LOCKOUT", "You can't remove your own admin access"));
        }
        if patch.role.is_some_and(|r| r != Role::Customer) && u.email.is_none() {
            return Err(AppError::validation("INVALID_EMAIL", "Add an email before giving this user partner or admin access"));
        }
        if let Some(v) = patch.name {
            u.name = v.trim().to_owned();
        }
        if let Some(v) = patch.role {
            u.role = v;
        }
        if let Some(v) = patch.status {
            u.status = v;
        }
        if let Some(v) = patch.restaurant_ids {
            u.restaurant_ids = v;
        }
        if u.role != Role::Owner {
            u.restaurant_ids.clear();
        }
        self.p.users.update(&u).await?;
        let action = match (before.status, u.status) {
            (UserStatus::Active, UserStatus::Blocked) => "user.blocked",
            (UserStatus::Blocked, UserStatus::Active) => "user.unblocked",
            _ => "user.updated",
        };
        let mut changes = vec![];
        if before.role != u.role {
            changes.push(format!("role {} → {}", before.role.as_str(), u.role.as_str()));
        }
        if before.restaurant_ids != u.restaurant_ids {
            changes.push(format!("restaurants {}", u.restaurant_ids.join(", ")));
        }
        if before.name != u.name {
            changes.push("name".to_owned());
        }
        self.audit_direct(None, "user", &u.id, action, |a| {
            a.from_status = Some(status_tag(&before.status));
            a.to_status = Some(status_tag(&u.status));
            a.note = (!changes.is_empty()).then(|| changes.join(" · "));
            a.subject_user_id = Some(u.id.clone());
        })
        .await?;
        Ok(u)
    }

    pub async fn admin_bookings(&self, user: &User) -> AppResult<Vec<Booking>> {
        self.require_role(user, &[Role::Admin])?;
        let tz = self.tz_by_restaurant().await?;
        Ok(self
            .p
            .bookings
            .list_recent(1000)
            .await?
            .into_iter()
            .filter(|b| b.status != BookingStatus::PendingPayment)
            .map(|b| {
                let z = tz.get(&b.restaurant.id).cloned().unwrap_or_default();
                self.effective_booking(b, &z)
            })
            .collect())
    }

    pub async fn admin_payments(&self, user: &User) -> AppResult<Vec<BillPayment>> {
        self.require_role(user, &[Role::Admin])?;
        self.p.billing.list_recent_payments(1000).await
    }

    pub async fn reset_demo_data(&self, user: &User) -> AppResult<()> {
        self.require_role(user, &[Role::Admin])?;
        match (&self.p.demo, self.settings.demo_mode) {
            (Some(d), true) => d.reset().await,
            _ => Err(AppError::not_found("Endpoint")),
        }
    }
}

/// Serialized enum tag, e.g. `RestaurantStatus::PendingApproval` → "PENDING_APPROVAL".
fn status_tag<T: serde::Serialize>(v: &T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}
