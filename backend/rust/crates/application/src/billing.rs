//! Bills and payments. The server owns all pricing: clients send amounts and
//! choices, never computed totals.

use std::collections::BTreeMap;

use feast_domain::deals::{self, ClaimOutcome, DealBudget, DEALS_USED_UP};
use feast_domain::market::money;
use feast_domain::model::*;
use feast_domain::pricing::{best_offer, offer_discount, payable_for, unit_of};
use feast_domain::{DomainEvent, EventEnvelope};

use crate::context::Feast;
use crate::error::{AppError, AppResult, ErrorKind};
use crate::ports::{ChargeOutcome, DealClaim, Settled, SlotRoom, WebhookEvent};
use feast_domain::refunds::RefundReason;

/// What a webhook did (the HTTP layer answers 200 in every case, so the
/// provider stops retrying).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookAck {
    Settled,
    Failed,
    AlreadySettled,
    Ignored,
}

impl Feast {
    pub async fn quote_bill(&self, user: &User, req: QuoteBillRequest) -> AppResult<Bill> {
        let r = self.public_restaurant(&req.restaurant_id).await?;
        if !r.detail.is_open_for(Service::BillPayment) {
            return Err(AppError::conflict("NOT_ACCEPTING", format!("{} isn't accepting bill payments on Feast right now", r.detail.name)));
        }
        // The diner enters the total from the restaurant's bill.
        let amount = req.bill_amount;
        // Sanity limits scale with the market's prices.
        let (market, _) = self.pricing_for(&r.detail).await?;
        let (max_bill, max_tip) = (market.price_bands[2] * 200, market.price_bands[2] * 2);
        if amount < unit_of(&market) {
            return Err(AppError::validation("INVALID_AMOUNT", "Enter your bill amount"));
        }
        if amount > max_bill {
            return Err(AppError::validation("INVALID_AMOUNT", "That amount looks too high — please check the bill"));
        }
        let tip = req.tip.unwrap_or(0);
        if !(0..=max_tip).contains(&tip) {
            return Err(AppError::validation("INVALID_TIP", format!("Tip must be between {} and {}", money(0, &market), money(max_tip, &market))));
        }

        // Feast deals are only for diners who booked on Feast (today, at this
        // restaurant) and pay on Feast: one discounted bill per booking.
        let booking = self.qualifying_booking(user, &r, req.booking_id.as_deref()).await?;
        let mut deal_notice = None;
        let mut offers: Vec<Offer> = Vec::new();
        match &booking {
            // A walk-in: no Feast booking, so the bill is paid at the restaurant as usual.
            None => return Err(AppError::conflict("BOOKING_REQUIRED", deals::BOOKING_REQUIRED)),
            Some(b) if self.p.billing.booking_deal_used(&b.id).await? => deal_notice = Some(deals::BOOKING_DEAL_USED.to_owned()),
            Some(b) => {
                if let Some(deal) = b.deal.clone() {
                    offers.push(Offer {
                        id: format!("{}_deal", b.id),
                        scope: OfferScope::Bill,
                        subtitle: Some(format!("Pre-booking deal · {}", b.code)),
                        ..deal
                    });
                }
                offers.extend(r.detail.offers.iter().filter(|o| o.scope == OfferScope::Bill).cloned());
            }
        }

        let mut chosen: Option<(Offer, Paise)> = match &req.offer_id {
            Some(id) => offers.iter().find(|o| &o.id == id).map(|o| (o.clone(), offer_discount(o, amount, &market))),
            None => best_offer(offers.iter(), amount, &market).map(|(o, d)| (o.clone(), d)),
        };
        // Restaurant-wide fairness controls: the per-bill cap and today's deal budget.
        if chosen.as_ref().is_some_and(|(_, d)| *d > 0) {
            let budget = self.deal_budget_for(&r).await?;
            if !deals::deal_available(budget.daily_deal_limit, budget.used) {
                chosen = None;
                deal_notice = Some(DEALS_USED_UP.to_owned());
            } else if let (Some((_, d)), Some(cap)) = (chosen.as_mut(), r.detail.settings.max_discount_per_bill) {
                let capped = deals::cap_discount(*d, Some(cap), &market);
                if capped < *d {
                    *d = capped;
                    deal_notice = Some(deals::cap_notice(cap, &market));
                }
            }
        }
        let discount = chosen.as_ref().map(|(_, d)| *d).unwrap_or(0);

        // The typed amount already includes taxes: no tax line is added.
        let mut lines = vec![BillLine { label: feast_domain::pricing::bill_total_label(&market.tax_name), amount, kind: BillLineKind::ItemTotal }];
        if let Some((o, d)) = &chosen {
            if *d > 0 {
                lines.push(BillLine { label: o.title.clone(), amount: -d, kind: BillLineKind::Discount });
            }
        }
        if tip > 0 {
            lines.push(BillLine { label: "Tip for staff".into(), amount: tip, kind: BillLineKind::Tip });
        }
        let bill = Bill {
            id: self.p.ids.new_id("bill"),
            restaurant_id: r.detail.id.clone(),
            currency: market.currency.clone(),
            bill_amount: amount,
            lines,
            applied_offer: chosen.filter(|(_, d)| *d > 0).map(|(o, _)| o),
            savings: discount,
            payable: payable_for(amount, discount, tip),
            deal_notice,
            booking_id: booking.map(|b| b.id),
        };
        self.p.billing.insert_bill(&bill).await?;
        Ok(bill)
    }

    pub async fn create_payment_intent(&self, user: &User, purpose: PaymentPurpose, reference_id: &str) -> AppResult<PaymentIntent> {
        let mut discounted = false;
        let mut booking_id = None;
        let (amount, restaurant_id) = match purpose {
            PaymentPurpose::Bill => {
                let bill = self.p.billing.get_bill(reference_id).await?.ok_or_else(|| AppError::not_found("Bill"))?;
                if self.p.billing.is_bill_paid(&bill.id).await? {
                    return Err(AppError::conflict("ALREADY_PAID", "This bill is already paid"));
                }
                if self.p.offline.latest_for_bill(&bill.id).await?.is_some_and(|o| feast_domain::offline::is_open(o.status)) {
                    return Err(AppError::conflict(
                        "OFFLINE_PENDING",
                        "You've already chosen to pay this bill at the counter. Wait for the restaurant to confirm it.",
                    ));
                }
                discounted = bill.savings > 0;
                booking_id = bill.booking_id.clone();
                (bill.payable, bill.restaurant_id)
            }
            PaymentPurpose::Booking => {
                let b = self.p.bookings.get(reference_id).await?.filter(|b| b.user_id == user.id).ok_or_else(|| AppError::not_found("Booking"))?;
                if b.status != BookingStatus::PendingPayment {
                    return Err(AppError::conflict("ALREADY_PAID", "This booking doesn't need a payment"));
                }
                (b.cover_charge, b.restaurant.id)
            }
            PaymentPurpose::Subscription => return Err(AppError::validation("WRONG_ENDPOINT", "Use the subscription checkout")),
        };
        let r = self.p.restaurants.get(&restaurant_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
        let (market, _) = self.pricing_for(&r.detail).await?;
        let intent_id = self.p.ids.new_id("pi");
        if discounted {
            // Hold one of today's deals for this bill before taking the money, so
            // concurrent payments can't go past the restaurant's daily limit.
            let claim = DealClaim {
                bill_id: reference_id.to_owned(),
                restaurant_id: r.detail.id.clone(),
                date: self.restaurant_today(&r).await?,
                intent_id: intent_id.clone(),
                claimed_at: self.p.clock.now_iso(),
                booking_id: booking_id.clone(),
                pending_offline: false,
            };
            let claimed_ev = self.event(
                Some(&r.detail.id),
                Some(&user.id),
                DomainEvent::DealClaimed { bill_id: reference_id.to_owned(), booking_id, intent_id: intent_id.clone(), offline: false },
            );
            match self.p.billing.claim_deal(&claim, r.detail.settings.daily_deal_limit, &self.hold_cutoff(), &[claimed_ev]).await? {
                ClaimOutcome::Claimed => {}
                ClaimOutcome::LimitReached => {
                    return Err(AppError::conflict(
                        "DEALS_USED_UP",
                        "Today's Feast deals at this restaurant were just used up. Check your bill again for the new total.",
                    ))
                }
                ClaimOutcome::BookingDealUsed => {
                    return Err(AppError::conflict(
                        "BOOKING_DEAL_USED",
                        "Your Feast deal for this booking is already used, or another payment with it is in progress. Check your bill again for the new total.",
                    ))
                }
            }
            let res = self.new_intent_with_id(intent_id.clone(), user, &market, purpose, reference_id, amount, None).await;
            if res.is_err() {
                let ev = self.deal_released(&r.detail.id, Some(&user.id), reference_id, &intent_id, "The payment couldn't start");
                self.p.billing.release_deal(reference_id, &intent_id, &[ev]).await?;
            }
            return res;
        }
        self.new_intent_with_id(intent_id, user, &market, purpose, reference_id, amount, None).await
    }

    /// The diner's Feast booking that lets them have a deal on today's bill
    /// here: the one they named if it qualifies, otherwise their best match
    /// (seated first, then the latest slot that has started, then the next one).
    async fn qualifying_booking(&self, user: &User, r: &Restaurant, booking_id: Option<&str>) -> AppResult<Option<Booking>> {
        let tz = self.city_tz(&r.detail.city_id).await?;
        let today = self.p.clock.local_date(&tz, 0);
        let qualifies = |b: &Booking| deals::booking_qualifies(b.status, &b.date, &today, b.user_id == user.id, b.restaurant.id == r.detail.id);
        if let Some(id) = booking_id {
            if let Some(b) = self.p.bookings.get(id).await?.map(|b| self.effective_booking(b, &tz)).filter(|b| qualifies(b)) {
                return Ok(Some(b));
            }
        }
        let now = self.p.clock.now_ms();
        let mut candidates: Vec<Booking> =
            self.p.bookings.list_by_user(&user.id).await?.into_iter().map(|b| self.effective_booking(b, &tz)).filter(|b| qualifies(b)).collect();
        candidates.sort_by_key(|b| {
            let at = self.p.clock.local_to_ms(&tz, &b.date, &b.time).unwrap_or(now);
            (b.status != BookingStatus::Seated, at > now, (at - now).abs())
        });
        Ok(candidates.into_iter().next())
    }

    /// The restaurant's local date (YYYY-MM-DD).
    async fn restaurant_today(&self, r: &Restaurant) -> AppResult<String> {
        let tz = self.city_tz(&r.detail.city_id).await?;
        Ok(self.p.clock.local_date(&tz, 0))
    }

    /// Today's deal budget for a restaurant (cap, limit, deals used so far).
    pub(crate) async fn deal_budget_for(&self, r: &Restaurant) -> AppResult<DealBudget> {
        let date = self.restaurant_today(r).await?;
        let used = self.p.billing.deals_used(&r.detail.id, &date, &self.hold_cutoff()).await?;
        let s = &r.detail.settings;
        Ok(DealBudget {
            date,
            daily_deal_limit: s.daily_deal_limit,
            max_discount_per_bill: s.max_discount_per_bill,
            used,
            remaining: deals::deals_left(s.daily_deal_limit, used),
        })
    }

    /// Owner/admin view of today's deal budget.
    pub async fn deal_budget(&self, user: &User, restaurant_id: &str) -> AppResult<DealBudget> {
        let r = self.managed_restaurant(user, restaurant_id).await?;
        self.deal_budget_for(&r).await
    }

    /// A payment intent routed to the market's payment provider (the only place
    /// that picks a provider — business code never names one).
    pub(crate) async fn new_intent(
        &self,
        user: &User,
        market: &Market,
        purpose: PaymentPurpose,
        reference_id: &str,
        amount: Paise,
        plan_id: Option<PlanId>,
    ) -> AppResult<PaymentIntent> {
        self.new_intent_with_id(self.p.ids.new_id("pi"), user, market, purpose, reference_id, amount, plan_id).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn new_intent_with_id(
        &self,
        id: String,
        user: &User,
        market: &Market,
        purpose: PaymentPurpose,
        reference_id: &str,
        amount: Paise,
        plan_id: Option<PlanId>,
    ) -> AppResult<PaymentIntent> {
        let gateway = self
            .p
            .payments
            .get(&market.payments.provider)
            .ok_or_else(|| AppError::new(ErrorKind::Unavailable, "PAYMENTS_UNAVAILABLE", "Payments aren't available right now. Please try again later."))?;
        let mut intent = PaymentIntent {
            id,
            purpose,
            reference_id: reference_id.into(),
            amount,
            currency: market.currency.clone(),
            // The gateway actually used (differs from the market's in test mode).
            provider: gateway.info().id,
            checkout: None,
            provider_ref: None,
            plan_id,
            user_id: Some(user.id.clone()),
            status: PaymentStatus::Created,
            result_ref: None,
            failure_reason: None,
            provider_payment_id: None,
        };
        let session = gateway.create_order(&intent).await?;
        let mut checkout = session.checkout;
        checkout.insert("methods".into(), market.payments.methods.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(","));
        intent.checkout = Some(checkout);
        intent.provider_ref = session.provider_ref;
        let ev = self.payment_started(&intent).await?;
        self.p.billing.insert_intent(&intent, &[ev]).await?;
        Ok(intent)
    }

    /// The browser reports the checkout finished: verify with the provider, then settle.
    pub async fn confirm_payment(
        &self,
        user: &User,
        intent_id: &str,
        method: PaymentMethod,
        simulate_failure: bool,
        payload: &BTreeMap<String, String>,
    ) -> AppResult<PaymentIntent> {
        let intent = self.p.billing.get_intent(intent_id).await?.ok_or_else(|| AppError::not_found("Payment"))?;
        // Only the person who started the payment (or an admin) may confirm it.
        if intent.user_id.as_deref().is_some_and(|u| u != user.id) && user.role != Role::Admin {
            return Err(AppError::not_found("Payment"));
        }
        if intent.status == PaymentStatus::Succeeded {
            return Ok(intent); // idempotent (maybe the webhook got here first)
        }
        let gateway = self
            .p
            .payments
            .get(&intent.provider)
            .ok_or_else(|| AppError::new(ErrorKind::Unavailable, "PAYMENTS_UNAVAILABLE", "Payments aren't available right now"))?;
        let outcome = gateway.charge(&intent, method, simulate_failure, payload).await?;
        if let ChargeOutcome::Failed(reason) = &outcome {
            if intent.purpose == PaymentPurpose::Subscription {
                self.subscription_payment_failed(&user.id, &intent, method, reason).await?;
            }
        }
        self.settle(intent, outcome, method).await
    }

    /// A provider webhook (`POST /v1/webhooks/payments/{provider}`): the
    /// provider tells us a payment succeeded or failed — even if the diner
    /// closed the browser before our confirm call. The signature is verified
    /// by the provider's adapter; amounts are checked against the intent.
    pub async fn payment_webhook(&self, provider: &str, headers: &[(String, String)], body: &[u8]) -> AppResult<WebhookAck> {
        crate::actor::set_webhook(provider);
        let gateway = self.p.payments.get(provider).filter(|g| g.info().id == provider).ok_or_else(|| AppError::not_found("Payment provider"))?;
        let event = gateway.parse_webhook(headers, body)?;
        let (provider_ref, outcome, amount, currency) = match event {
            WebhookEvent::Payment { provider_ref, outcome, amount, currency } => (provider_ref, outcome, amount, currency),
            WebhookEvent::Refund { provider_refund_id, status, failure } => {
                let changed = self.apply_refund_webhook(provider, &provider_refund_id, status, failure).await?;
                return Ok(if changed { WebhookAck::Settled } else { WebhookAck::Ignored });
            }
            WebhookEvent::Ignored(_) => return Ok(WebhookAck::Ignored),
        };
        let Some(intent) = self.p.billing.find_intent_by_provider_ref(provider, &provider_ref).await? else {
            tracing::warn!(provider, %provider_ref, "webhook for an unknown payment; ignoring");
            return Ok(WebhookAck::Ignored);
        };
        if amount.is_some_and(|a| a != intent.amount) || currency.as_deref().is_some_and(|c| !c.eq_ignore_ascii_case(&intent.currency)) {
            tracing::error!(intent = %intent.id, ?amount, ?currency, "webhook amount/currency doesn't match the intent; not settling");
            return Ok(WebhookAck::Ignored);
        }
        if intent.status == PaymentStatus::Succeeded {
            return Ok(WebhookAck::AlreadySettled);
        }
        let fallback = self.default_method().await;
        let succeeded = matches!(outcome, ChargeOutcome::Succeeded { .. });
        let settled = self.settle(intent, outcome, fallback).await?;
        tracing::info!(intent = %settled.id, provider, status = ?settled.status, "payment settled by webhook");
        Ok(if succeeded { WebhookAck::Settled } else { WebhookAck::Failed })
    }

    /// Method to record when a webhook doesn't say (the market's first).
    async fn default_method(&self) -> PaymentMethod {
        let market = self.p.markets.get_market(&self.settings.country).await.ok().flatten();
        market.and_then(|m| m.payments.methods.first().copied()).unwrap_or(PaymentMethod::Card)
    }

    /// Apply a provider's verdict to an intent. Success is applied exactly once:
    /// the repository *claims* the intent in the same transaction as the effects,
    /// so a racing confirm + webhook can't double-confirm a booking, record a
    /// bill twice or extend a subscription twice.
    async fn settle(&self, mut intent: PaymentIntent, outcome: ChargeOutcome, method: PaymentMethod) -> AppResult<PaymentIntent> {
        match outcome {
            ChargeOutcome::Failed(reason) => {
                intent.status = PaymentStatus::Failed;
                intent.failure_reason = Some(reason.clone());
                let restaurant_id = self.restaurant_of(&intent).await?;
                let failed = self.event(
                    restaurant_id.as_deref(),
                    intent.user_id.as_deref(),
                    DomainEvent::PaymentFailed {
                        intent_id: intent.id.clone(),
                        purpose: intent.purpose,
                        reference_id: intent.reference_id.clone(),
                        amount: intent.amount,
                        currency: intent.currency.clone(),
                        reason: Some(reason),
                    },
                );
                self.p.billing.update_intent(&intent, &[failed]).await?; // no-op if it already succeeded
                if intent.purpose == PaymentPurpose::Bill {
                    // Give the deal back to today's budget (no-op if this intent no longer holds it).
                    let ev = self.deal_released(
                        restaurant_id.as_deref().unwrap_or_default(),
                        intent.user_id.as_deref(),
                        &intent.reference_id,
                        &intent.id,
                        "The payment failed",
                    );
                    self.p.billing.release_deal(&intent.reference_id, &intent.id, &[ev]).await?;
                }
                self.p.billing.get_intent(&intent.id).await?.ok_or_else(|| AppError::not_found("Payment"))
            }
            ChargeOutcome::Succeeded { method: reported, provider_payment_id } => {
                let method = reported.unwrap_or(method);
                let payer = intent.user_id.clone().unwrap_or_default();
                intent.status = PaymentStatus::Succeeded;
                intent.failure_reason = None;
                if provider_payment_id.is_some() {
                    intent.provider_payment_id = provider_payment_id;
                }
                let paid = PaymentRef {
                    intent_id: intent.id.clone(),
                    method,
                    amount: intent.amount,
                    currency: intent.currency.clone(),
                    provider: intent.provider.clone(),
                    paid_at: self.p.clock.now_iso(),
                    settlement: Settlement::Online,
                };
                let settled = match intent.purpose {
                    PaymentPurpose::Booking => {
                        let mut b = self.p.bookings.get(&intent.reference_id).await?.ok_or_else(|| AppError::not_found("Booking"))?;
                        let r = self.p.restaurants.get(&b.restaurant.id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
                        // The booking is confirmed only if it still awaits payment and
                        // (when its hold lapsed) a table is still free — checked again
                        // under the slot lock. A cancelled booking is never brought back.
                        let room = SlotRoom { tables: feast_domain::slots::feast_tables(&r, &b.date, &b.time), pending_since: self.hold_cutoff() };
                        b.status = BookingStatus::Confirmed;
                        b.payment = Some(paid);
                        intent.result_ref = Some(b.id.clone());
                        // A paid booking is now confirmed: tell the restaurant.
                        let events = vec![
                            self.event(Some(&b.restaurant.id), Some(&b.user_id), DomainEvent::BookingCreated { booking: b.clone() }),
                            self.event(
                                Some(&b.restaurant.id),
                                Some(&b.user_id),
                                DomainEvent::PaymentSucceeded {
                                    bill_payment_id: None,
                                    booking_id: Some(b.id.clone()),
                                    amount: intent.amount,
                                    currency: intent.currency.clone(),
                                    offline: false,
                                },
                            ),
                        ];
                        match self.p.billing.complete_booking_payment(&intent, &b, &room, &events).await? {
                            Settled::NotApplied => {
                                return self
                                    .payment_not_applied(
                                        intent,
                                        RefundReason::PaidAfterCancellation,
                                        "The booking was no longer waiting for this payment (cancelled, expired or its table taken).",
                                    )
                                    .await
                            }
                            other => other,
                        }
                    }
                    PaymentPurpose::Bill => {
                        let bill = self.p.billing.get_bill(&intent.reference_id).await?.ok_or_else(|| AppError::not_found("Bill"))?;
                        let r = self.p.restaurants.get(&bill.restaurant_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
                        let bp = BillPayment {
                            id: self.p.ids.new_id("bp"),
                            user_id: payer.clone(),
                            bill: bill.clone(),
                            restaurant: r.detail.reference(),
                            payment: paid,
                        };
                        intent.result_ref = Some(bp.id.clone());
                        let events = self.bill_paid_events(&bill, &bp, &payer);
                        match self.p.billing.complete_bill_payment(&intent, &bp, &events).await? {
                            Settled::NotApplied => {
                                return self.payment_not_applied(intent, RefundReason::AlreadyPaid, "The bill was already paid by another payment.").await
                            }
                            other => other,
                        }
                    }
                    PaymentPurpose::Subscription => {
                        if self.subscription_paid(&payer, &mut intent, method).await? {
                            Settled::Applied
                        } else {
                            Settled::AlreadySettled
                        }
                    }
                };
                if settled == Settled::AlreadySettled {
                    tracing::info!(intent = %intent.id, "payment was already settled (confirm/webhook race); nothing applied twice");
                }
                self.p.billing.get_intent(&intent.id).await?.ok_or_else(|| AppError::not_found("Payment"))
            }
        }
    }

    /// `payment.succeeded` (+ `deal.used` when the bill had a Feast deal) for a paid bill.
    pub(crate) fn bill_paid_events(&self, bill: &Bill, bp: &BillPayment, payer: &str) -> Vec<EventEnvelope> {
        let mut events = vec![self.event(
            Some(&bill.restaurant_id),
            Some(payer),
            DomainEvent::PaymentSucceeded {
                bill_payment_id: Some(bp.id.clone()),
                booking_id: None,
                amount: bp.payment.amount,
                currency: bp.payment.currency.clone(),
                offline: bp.payment.settlement == Settlement::Offline,
            },
        )];
        if bill.savings > 0 {
            events.push(self.event(
                Some(&bill.restaurant_id),
                Some(payer),
                DomainEvent::DealUsed {
                    bill_id: bill.id.clone(),
                    booking_id: bill.booking_id.clone(),
                    discount: bill.savings,
                    currency: bill.currency.clone(),
                },
            ));
        }
        events
    }

    pub(crate) fn deal_released(&self, restaurant_id: &str, user_id: Option<&str>, bill_id: &str, intent_id: &str, reason: &str) -> EventEnvelope {
        self.event(
            Some(restaurant_id).filter(|r| !r.is_empty()),
            user_id,
            DomainEvent::DealReleased { bill_id: bill_id.to_owned(), intent_id: intent_id.to_owned(), reason: reason.to_owned() },
        )
    }

    /// The restaurant an intent is for (audit/notification routing).
    pub(crate) async fn restaurant_of(&self, intent: &PaymentIntent) -> AppResult<Option<String>> {
        Ok(match intent.purpose {
            PaymentPurpose::Subscription => Some(intent.reference_id.clone()),
            PaymentPurpose::Bill => self.p.billing.get_bill(&intent.reference_id).await?.map(|b| b.restaurant_id),
            PaymentPurpose::Booking => self.p.bookings.get(&intent.reference_id).await?.map(|b| b.restaurant.id),
        })
    }

    pub(crate) async fn payment_started(&self, intent: &PaymentIntent) -> AppResult<EventEnvelope> {
        let restaurant_id = self.restaurant_of(intent).await?;
        Ok(self.event(
            restaurant_id.as_deref(),
            intent.user_id.as_deref(),
            DomainEvent::PaymentStarted {
                intent_id: intent.id.clone(),
                purpose: intent.purpose,
                reference_id: intent.reference_id.clone(),
                amount: intent.amount,
                currency: intent.currency.clone(),
                provider: intent.provider.clone(),
            },
        ))
    }

    pub async fn bill_payment(&self, user: &User, id: &str) -> AppResult<BillPayment> {
        let bp = self.p.billing.get_bill_payment(id).await?.ok_or_else(|| AppError::not_found("Payment"))?;
        if bp.user_id == user.id || user.can_manage(&bp.restaurant.id) {
            Ok(bp)
        } else {
            Err(AppError::not_found("Payment"))
        }
    }

    pub async fn my_bill_payments(&self, user: &User) -> AppResult<Vec<BillPayment>> {
        let mut list = self.p.billing.payments_by_user(&user.id).await?;
        list.sort_by(|a, b| b.payment.paid_at.cmp(&a.payment.paid_at));
        Ok(list)
    }
}
