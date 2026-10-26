//! Insights screens: the owner scorecard ("did Feast pay for itself?"), the
//! diner's savings page and the admin marketplace health page. This module only
//! gathers data through the ports; the arithmetic lives in `feast_domain::insights`.

use std::collections::{BTreeMap, HashMap, HashSet};

use feast_domain::insights::{self as ins, *};
use feast_domain::model::*;

use crate::context::Feast;
use crate::error::{AppError, AppResult};

const TOP_RESTAURANTS: usize = 5;

fn check_period(days: i64, allowed: &[i64]) -> AppResult<()> {
    if allowed.contains(&days) {
        Ok(())
    } else {
        let list = allowed.iter().map(|d| d.to_string()).collect::<Vec<_>>().join(", ");
        Err(AppError::validation("INVALID_PERIOD", format!("Choose a period of {list} days")))
    }
}

fn in_range(date: &str, from: Option<&str>, to: &str) -> bool {
    from.is_none_or(|f| date >= f) && date <= to
}

impl Feast {
    /// Owner scorecard for the last `days` local days (today included).
    pub async fn owner_insights(&self, user: &User, restaurant_id: &str, days: i64) -> AppResult<OwnerInsights> {
        check_period(days, &OWNER_PERIODS)?;
        let r = self.managed_restaurant(user, restaurant_id).await?;
        let tz = self.city_tz(&r.detail.city_id).await?;
        let clock = &self.p.clock;
        let to = clock.local_date(&tz, 0);
        let from = clock.local_date(&tz, -(days - 1));
        let now = self.now();
        let window_start = now - chrono::Duration::days(days);

        // Every booking and bill ever at this restaurant: needed to tell new diners from repeat ones.
        let bookings: Vec<Booking> = self
            .p
            .bookings
            .list_by_restaurant_between(restaurant_id, "0000-01-01", "9999-12-31")
            .await?
            .into_iter()
            .map(|b| self.effective_booking(b, &tz))
            .collect();
        let pays = self.p.billing.payments_by_restaurant(restaurant_id).await?;
        let pay_date = |p: &BillPayment| clock.local_date_of(&tz, &p.payment.paid_at).unwrap_or_default();

        let honoured = |b: &Booking| matches!(b.status, BookingStatus::Seated | BookingStatus::Completed);
        let all_visits: Vec<Visit> = unique_visits(
            bookings
                .iter()
                .filter(|b| honoured(b))
                .map(|b| Visit { user_id: b.user_id.clone(), restaurant_id: restaurant_id.to_owned(), date: b.date.clone() })
                .chain(pays.iter().map(|p| Visit { user_id: p.user_id.clone(), restaurant_id: restaurant_id.to_owned(), date: pay_date(p) })),
        );
        let visits: Vec<&Visit> = all_visits.iter().filter(|v| in_range(&v.date, Some(&from), &to)).collect();
        let (new_diners, repeat_diners) = new_and_repeat(&all_visits, &from, &to);

        let period_bookings: Vec<&Booking> = bookings.iter().filter(|b| in_range(&b.date, Some(&from), &to)).collect();
        let mut funnel = BookingFunnel::default();
        for b in &period_bookings {
            funnel.add(b, &to);
        }
        let kept: Vec<&Booking> = period_bookings.iter().copied().filter(|b| honoured(b)).collect();
        let cover_charges = period_bookings.iter().filter(|b| b.status != BookingStatus::Cancelled).filter_map(|b| b.payment.as_ref().map(|p| p.amount)).sum();

        let period_pays: Vec<(&BillPayment, String)> = pays.iter().map(|p| (p, pay_date(p))).filter(|(_, d)| in_range(d, Some(&from), &to)).collect();
        let online = |p: &BillPayment| p.payment.settlement == Settlement::Online;
        let revenue: Paise = period_pays.iter().filter(|(p, _)| online(p)).map(|(p, _)| p.payment.amount).sum();
        let offline_revenue: Paise = period_pays.iter().filter(|(p, _)| !online(p)).map(|(p, _)| p.payment.amount).sum();
        let online_bills = period_pays.iter().filter(|(p, _)| online(p)).count();
        let with_deal: Vec<&BillPayment> = period_pays.iter().map(|(p, _)| *p).filter(|p| p.bill.savings > 0).collect();
        let deals = deal_usage(
            with_deal.len(),
            with_deal.iter().map(|p| p.bill.savings).sum(),
            r.detail.settings.daily_deal_limit,
            r.detail.settings.max_discount_per_bill,
            days,
        );

        // What Feast cost: subscription invoices spread over the days they cover.
        let feast_cost: Paise = self
            .p
            .subscriptions
            .payments_for(restaurant_id)
            .await?
            .iter()
            .filter(|p| p.status == PlatformPaymentStatus::Paid)
            .map(|p| prorated(p.amount, &p.period_start, &p.period_end, window_start, now))
            .sum();

        let revenue_by_date: Vec<(String, Paise)> = period_pays.iter().filter(|(p, _)| online(p)).map(|(p, d)| (d.clone(), p.payment.amount)).collect();
        let active: Vec<&Booking> = period_bookings.iter().copied().filter(|b| b.status != BookingStatus::Cancelled).collect();

        let reviews: Vec<(String, i64)> =
            self.p.reviews.reviews_for_restaurant(restaurant_id, true, 2000).await?.into_iter().map(|rv| (rv.created_at, rv.rating)).collect();
        let cases = self.p.cases.cases_for_restaurant(restaurant_id).await?;
        let case_date = |c: &feast_domain::trust::Case| clock.local_date_of(&tz, &c.created_at).unwrap_or_default();

        Ok(OwnerInsights {
            restaurant: r.detail.reference(),
            currency: r.detail.currency.clone(),
            period: InsightsPeriod { days, from: Some(from.clone()), to: to.clone() },
            feast_diners: distinct(visits.iter().map(|v| v.user_id.as_str())),
            new_diners,
            repeat_diners,
            visits: visits.len(),
            covers: kept.iter().map(|b| b.guests).sum(),
            no_show_rate: funnel.no_show_rate(),
            bookings: funnel,
            revenue,
            bills_paid: online_bills,
            average_bill: average(revenue, online_bills),
            offline_revenue,
            offline_bills: period_pays.len() - online_bills,
            cover_charges,
            deals,
            feast_cost,
            return_on_cost: (feast_cost > 0).then(|| revenue as f64 / feast_cost as f64),
            by_weekday: by_weekday(active.iter().copied(), &revenue_by_date),
            by_hour: by_hour(active.iter().copied()),
            rating: r.detail.rating,
            rating_count: r.detail.rating_count,
            rating_trend: rating_trend(&reviews, &to[..7], 6),
            complaints_opened: cases.iter().filter(|c| in_range(&case_date(c), Some(&from), &to)).count(),
            open_cases: cases.iter().filter(|c| ins::is_open(c)).count(),
        })
    }

    /// A diner's own savings and dining history; `days` = 0 for all time.
    pub async fn diner_insights(&self, user: &User, days: i64) -> AppResult<DinerInsights> {
        self.require_role(user, &[Role::Customer])?;
        check_period(days, &DINER_PERIODS)?;
        let tzs = self.tz_by_restaurant().await?;
        let tz_of = |rid: &str| tzs.get(rid).cloned().unwrap_or_default();
        let clock = &self.p.clock;
        // The diner's "today" is the one of their deployment's first city; good enough for date ranges.
        let home_tz = self.p.markets.list_cities().await?.into_iter().next().map(|c| c.timezone).unwrap_or_default();
        let to = clock.local_date(&home_tz, 0);
        let from = (days > 0).then(|| clock.local_date(&home_tz, -(days - 1)));
        let from_ref = from.as_deref();

        let bookings: Vec<Booking> = self
            .p
            .bookings
            .list_by_user(&user.id)
            .await?
            .into_iter()
            .map(|b| {
                let z = tz_of(&b.restaurant.id);
                self.effective_booking(b, &z)
            })
            .filter(|b| in_range(&b.date, from_ref, &to) || b.date > to)
            .collect();
        let pays: Vec<(BillPayment, String)> = self
            .p
            .billing
            .payments_by_user(&user.id)
            .await?
            .into_iter()
            .map(|p| {
                let d = clock.local_date_of(&tz_of(&p.restaurant.id), &p.payment.paid_at).unwrap_or_default();
                (p, d)
            })
            .filter(|(_, d)| in_range(d, from_ref, &to))
            .collect();

        let mut funnel = BookingFunnel::default();
        for b in &bookings {
            funnel.add(b, &to);
        }
        let honoured: Vec<&Booking> =
            bookings.iter().filter(|b| matches!(b.status, BookingStatus::Seated | BookingStatus::Completed) && b.date <= to).collect();
        let visits = unique_visits(
            honoured
                .iter()
                .map(|b| Visit { user_id: user.id.clone(), restaurant_id: b.restaurant.id.clone(), date: b.date.clone() })
                .chain(pays.iter().map(|(p, d)| Visit { user_id: user.id.clone(), restaurant_id: p.restaurant.id.clone(), date: d.clone() })),
        );
        let covers_paid: Vec<(&Booking, Paise)> = bookings
            .iter()
            .filter(|b| b.status != BookingStatus::Cancelled && b.date <= to)
            .filter_map(|b| b.payment.as_ref().map(|p| (b, p.amount)))
            .collect();

        // Per restaurant and per month.
        let mut refs: HashMap<String, RestaurantRef> = HashMap::new();
        let mut per_restaurant: HashMap<String, (usize, Paise, Paise)> = HashMap::new();
        let mut per_month: BTreeMap<String, (Paise, Paise)> = BTreeMap::new();
        for v in &visits {
            per_restaurant.entry(v.restaurant_id.clone()).or_default().0 += 1;
        }
        for (p, d) in &pays {
            refs.entry(p.restaurant.id.clone()).or_insert_with(|| p.restaurant.clone());
            let e = per_restaurant.entry(p.restaurant.id.clone()).or_default();
            e.1 += p.payment.amount;
            e.2 += p.bill.savings;
            let m = per_month.entry(d[..7.min(d.len())].to_owned()).or_default();
            m.0 += p.payment.amount;
            m.1 += p.bill.savings;
        }
        for (b, amount) in &covers_paid {
            refs.entry(b.restaurant.id.clone()).or_insert_with(|| b.restaurant.clone());
            per_restaurant.entry(b.restaurant.id.clone()).or_default().1 += amount;
            per_month.entry(b.date[..7.min(b.date.len())].to_owned()).or_default().0 += amount;
        }
        for b in &honoured {
            refs.entry(b.restaurant.id.clone()).or_insert_with(|| b.restaurant.clone());
        }
        let mut favourites: Vec<FavouriteRestaurant> = per_restaurant
            .iter()
            .filter(|(_, (visits, _, _))| *visits > 0)
            .filter_map(|(rid, (visits, spent, saved))| {
                refs.get(rid).map(|r| FavouriteRestaurant { restaurant: r.clone(), visits: *visits, spent: *spent, saved: *saved })
            })
            .collect();
        favourites.sort_by(|a, b| b.visits.cmp(&a.visits).then_with(|| b.spent.cmp(&a.spent)).then_with(|| a.restaurant.name.cmp(&b.restaurant.name)));
        favourites.truncate(3);

        let mut cuisines = vec![];
        for v in &visits {
            if let Some(r) = self.p.restaurants.get(&v.restaurant_id).await? {
                cuisines.extend(r.detail.cuisines.iter().cloned());
            }
        }

        let reviews_written =
            self.p.reviews.reviews_by_user(&user.id).await?.iter().filter(|rv| in_range(&rv.created_at[..10.min(rv.created_at.len())], from_ref, &to)).count();
        let currency = match pays.first().map(|(p, _)| p.payment.currency.clone()).or_else(|| bookings.first().map(|b| b.currency.clone())) {
            Some(c) if !c.is_empty() => c,
            _ => self.p.markets.list_markets().await?.into_iter().next().map(|m| m.currency).unwrap_or_default(),
        };

        Ok(DinerInsights {
            currency,
            period: InsightsPeriod { days, from, to },
            total_saved: pays.iter().map(|(p, _)| p.bill.savings).sum(),
            total_spent: pays.iter().map(|(p, _)| p.payment.amount).sum::<Paise>() + covers_paid.iter().map(|(_, a)| a).sum::<Paise>(),
            bills_paid: pays.len(),
            visits: visits.len(),
            restaurants_visited: distinct(visits.iter().map(|v| v.restaurant_id.as_str())),
            bookings: funnel,
            reviews_written,
            favourite_restaurants: favourites,
            favourite_cuisines: top_counts(cuisines, 3),
            by_month: per_month.into_iter().map(|(month, (spent, saved))| MonthMoney { month, spent, saved }).collect(),
        })
    }

    /// Marketplace health for the last `days` UTC days (today included).
    pub async fn admin_insights(&self, user: &User, days: i64) -> AppResult<AdminInsights> {
        self.require_role(user, &[Role::Admin])?;
        check_period(days, &OWNER_PERIODS)?;
        let now = self.now();
        let to = now.format("%Y-%m-%d").to_string();
        let from_day = now - chrono::Duration::days(days - 1);
        let from = from_day.format("%Y-%m-%d").to_string();
        let since = format!("{from}T00:00:00.000Z");
        let tzs = self.tz_by_restaurant().await?;
        let tz_of = |rid: &str| tzs.get(rid).cloned().unwrap_or_default();

        let restaurants = self.p.restaurants.list().await?;
        let by_id: HashMap<&str, &Restaurant> = restaurants.iter().map(|r| (r.detail.id.as_str(), r)).collect();
        let count = |s: &[RestaurantStatus]| restaurants.iter().filter(|r| s.contains(&r.detail.status)).count();
        let pipeline = RestaurantPipeline {
            live: count(&[RestaurantStatus::Live]),
            pending_approval: count(&[RestaurantStatus::PendingApproval]),
            onboarding: count(&[RestaurantStatus::Onboarding]),
            paused_or_suspended: count(&[RestaurantStatus::Paused, RestaurantStatus::Suspended]),
        };

        let bookings: Vec<Booking> = self
            .p
            .bookings
            .list_between(&from, &to)
            .await?
            .into_iter()
            .map(|b| {
                let z = tz_of(&b.restaurant.id);
                self.effective_booking(b, &z)
            })
            .collect();
        let mut funnel = BookingFunnel::default();
        for b in &bookings {
            funnel.add(b, &to);
        }
        let pays = self.p.billing.payments_since(&since).await?;

        let visits = unique_visits(
            bookings
                .iter()
                .filter(|b| matches!(b.status, BookingStatus::Seated | BookingStatus::Completed))
                .map(|b| Visit { user_id: b.user_id.clone(), restaurant_id: b.restaurant.id.clone(), date: b.date.clone() })
                .chain(pays.iter().map(|p| Visit {
                    user_id: p.user_id.clone(),
                    restaurant_id: p.restaurant.id.clone(),
                    date: p.payment.paid_at[..10.min(p.payment.paid_at.len())].to_owned(),
                })),
        );

        // Money, never summed across currencies.
        let subs = self.subscription_metrics().await?;
        let mut money: BTreeMap<String, MarketMoney> = BTreeMap::new();
        let entry = |money: &mut BTreeMap<String, MarketMoney>, c: &str| {
            money.entry(c.to_owned()).or_insert_with(|| MarketMoney {
                currency: c.to_owned(),
                gmv: 0,
                bills_paid: 0,
                average_bill: None,
                discounts: 0,
                cover_charges: 0,
                subscription_revenue: 0,
                mrr: 0,
                offline_settled: 0,
                offline_bills: 0,
            });
        };
        for m in self.p.markets.list_markets().await? {
            entry(&mut money, &m.currency);
        }
        for p in &pays {
            entry(&mut money, &p.payment.currency);
            let e = money.get_mut(&p.payment.currency).expect("entry");
            if p.payment.settlement == Settlement::Offline {
                e.offline_settled += p.payment.amount;
                e.offline_bills += 1;
            } else {
                e.gmv += p.payment.amount;
                e.bills_paid += 1;
            }
            e.discounts += p.bill.savings;
        }
        for b in bookings.iter().filter(|b| b.status != BookingStatus::Cancelled) {
            if let Some(p) = &b.payment {
                entry(&mut money, &p.currency);
                money.get_mut(&p.currency).expect("entry").cover_charges += p.amount;
            }
        }
        for p in
            self.p.subscriptions.all_payments().await?.iter().filter(|p| p.status == PlatformPaymentStatus::Paid && p.created_at.as_str() >= since.as_str())
        {
            entry(&mut money, &p.currency);
            money.get_mut(&p.currency).expect("entry").subscription_revenue += p.amount;
        }
        for r in &subs.revenue {
            entry(&mut money, &r.currency);
            money.get_mut(&r.currency).expect("entry").mrr = r.mrr;
        }
        for m in money.values_mut() {
            m.average_bill = average(m.gmv, m.bills_paid);
        }

        let cases = self.p.cases.all_cases().await?;
        let opened = cases.iter().filter(|c| c.created_at.as_str() >= since.as_str()).count();
        let decided: Vec<bool> =
            cases.iter().filter(|c| c.outcome.as_ref().is_some_and(|o| o.at.as_str() >= since.as_str())).filter_map(decided_on_time).collect();
        let disputes = DisputeStats {
            opened,
            per_100_visits: rate(opened, visits.len()).map(|r| (r * 1000.0).round() / 10.0),
            decided_by_feast: decided.len(),
            decided_on_time: rate(decided.iter().filter(|ok| **ok).count(), decided.len()),
            with_feast_now: cases.iter().filter(|c| c.status == feast_domain::trust::CaseStatus::WithFeast).count(),
            open_now: cases.iter().filter(|c| ins::is_open(c)).count(),
        };

        let reviews = self.p.reviews.reviews_since(&since).await?;
        let published: Vec<i64> = reviews.iter().filter(|r| r.status == feast_domain::trust::ReviewStatus::Published).map(|r| r.rating).collect();
        let review_health = ReviewHealth {
            posted: reviews.len(),
            average: (!published.is_empty()).then(|| (published.iter().sum::<i64>() as f64 / published.len() as f64 * 10.0).round() / 10.0),
            hidden: reviews.len() - published.len(),
        };

        let markets: HashMap<String, String> = self.p.markets.list_markets().await?.into_iter().map(|m| (m.country_code, m.currency)).collect();
        let cities_list = self.p.markets.list_cities().await?;
        let city_name: HashMap<&str, &str> = cities_list.iter().map(|c| (c.id.as_str(), c.name.as_str())).collect();
        let bookings_by_restaurant = |rid: &str| bookings.iter().filter(|b| b.restaurant.id == rid && b.status != BookingStatus::Cancelled).count();
        let gmv_by_restaurant =
            |rid: &str| pays.iter().filter(|p| p.restaurant.id == rid && p.payment.settlement == Settlement::Online).map(|p| p.payment.amount).sum::<Paise>();
        let mut cities: Vec<CityHealth> = cities_list
            .iter()
            .map(|c| {
                let ids: HashSet<&str> = restaurants.iter().filter(|r| r.detail.city_id == c.id).map(|r| r.detail.id.as_str()).collect();
                CityHealth {
                    city_id: c.id.clone(),
                    city_name: c.name.clone(),
                    currency: markets.get(&c.country_code).cloned().unwrap_or_default(),
                    live_restaurants: restaurants.iter().filter(|r| r.detail.city_id == c.id && r.detail.status == RestaurantStatus::Live).count(),
                    bookings: ids.iter().map(|id| bookings_by_restaurant(id)).sum(),
                    gmv: ids.iter().map(|id| gmv_by_restaurant(id)).sum(),
                }
            })
            .collect();
        cities
            .sort_by(|a, b| b.bookings.cmp(&a.bookings).then_with(|| b.live_restaurants.cmp(&a.live_restaurants)).then_with(|| a.city_name.cmp(&b.city_name)));

        let mut top: Vec<TopRestaurant> = by_id
            .values()
            .filter(|r| r.detail.status == RestaurantStatus::Live)
            .map(|r| TopRestaurant {
                restaurant: r.detail.reference(),
                city_name: city_name.get(r.detail.city_id.as_str()).map(|s| (*s).to_owned()).unwrap_or_default(),
                currency: r.detail.currency.clone(),
                bookings: bookings_by_restaurant(&r.detail.id),
                gmv: gmv_by_restaurant(&r.detail.id),
                rating: r.detail.rating,
            })
            .filter(|t| t.bookings > 0 || t.gmv > 0)
            .collect();
        top.sort_by(|a, b| b.bookings.cmp(&a.bookings).then_with(|| b.gmv.cmp(&a.gmv)).then_with(|| a.restaurant.name.cmp(&b.restaurant.name)));
        top.truncate(TOP_RESTAURANTS);

        let new_diners = self.p.users.list().await?.iter().filter(|u| u.role == Role::Customer && u.created_at.as_str() >= since.as_str()).count();

        Ok(AdminInsights {
            period: InsightsPeriod { days, from: Some(from), to },
            restaurants: pipeline,
            subscriptions: subs,
            money: money.into_values().collect(),
            no_show_rate: funnel.no_show_rate(),
            bookings: funnel,
            active_diners: distinct(visits.iter().map(|v| v.user_id.as_str())),
            visits: visits.len(),
            new_diners,
            disputes,
            reviews: review_health,
            cities,
            top_restaurants: top,
        })
    }
}
