//! Paying at the counter (cash or the restaurant's own card terminal).
//!
//! Feast never holds that money, so the bill only counts as paid once the
//! restaurant confirms it (rules: `feast_domain::offline`). A booked diner
//! still gets their Feast deal: it is held while the payment waits, used on
//! approval, and freed on rejection or expiry. Every step is an event, so it
//! lands in the audit trail with who did it.

use chrono::Duration;
use feast_domain::deals::ClaimOutcome;
use feast_domain::model::*;
use feast_domain::offline::{self as rules, Decision, OfflineError};
use feast_domain::subscription::iso;
use feast_domain::DomainEvent;

use crate::actor;
use crate::context::Feast;
use crate::error::{AppError, AppResult};
use crate::ports::DealClaim;

/// The internal "provider" of counter payments (never a real gateway).
pub const OFFLINE_PROVIDER: &str = "offline";

fn rule_error(e: OfflineError) -> AppError {
    let msg = match &e {
        OfflineError::AlreadyDecided(s) => format!("This payment was already {}", s.as_str().to_lowercase().replace('_', " ")),
        OfflineError::ReasonRequired => format!("Tell the diner why (at least {} characters)", rules::MIN_REASON_CHARS),
        OfflineError::WrongAmount { .. } => "The amount received must match the bill. Reject it if the diner paid a different amount.".to_owned(),
        OfflineError::NotExpiredYet => "This payment hasn't expired yet".to_owned(),
    };
    AppError::conflict(e.code(), msg)
}

impl Feast {
    /// A diner says they'll pay (or paid) this bill at the counter.
    pub async fn submit_offline_payment(&self, user: &User, bill_id: &str, method: PaymentMethod) -> AppResult<OfflinePayment> {
        if !method.is_offline() {
            return Err(AppError::validation("INVALID_METHOD", "Choose cash or card at the counter"));
        }
        let bill = self.p.billing.get_bill(bill_id).await?.ok_or_else(|| AppError::not_found("Bill"))?;
        if self.p.billing.is_bill_paid(&bill.id).await? {
            return Err(AppError::conflict("ALREADY_PAID", "This bill is already paid"));
        }
        if let Some(open) = self.p.offline.latest_for_bill(&bill.id).await?.filter(|o| rules::is_open(o.status)) {
            return if open.user_id == user.id {
                Ok(open) // idempotent: the same diner tapped twice
            } else {
                Err(AppError::conflict("OFFLINE_PENDING", "Someone already chose to pay this bill at the counter"))
            };
        }
        let r = self.p.restaurants.get(&bill.restaurant_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
        let now = self.now();
        let intent = PaymentIntent {
            id: self.p.ids.new_id("pi"),
            purpose: PaymentPurpose::Bill,
            reference_id: bill.id.clone(),
            amount: bill.payable,
            currency: bill.currency.clone(),
            provider: OFFLINE_PROVIDER.into(),
            checkout: None,
            provider_ref: None,
            plan_id: None,
            user_id: Some(user.id.clone()),
            status: PaymentStatus::Created,
            result_ref: None,
            failure_reason: None,
        };
        let payment = OfflinePayment {
            id: self.p.ids.new_id("op"),
            bill_id: bill.id.clone(),
            intent_id: intent.id.clone(),
            restaurant: r.detail.reference(),
            user_id: user.id.clone(),
            diner_name: user.name.clone(),
            method,
            amount: bill.payable,
            currency: bill.currency.clone(),
            savings: bill.savings,
            booking_id: bill.booking_id.clone(),
            status: OfflinePaymentStatus::PendingApproval,
            submitted_at: iso(now),
            expires_at: iso(now + Duration::minutes(rules::APPROVAL_WINDOW_MINUTES)),
            decided_at: None,
            decided_by: None,
            reason: None,
            amount_received: None,
            bill_payment_id: None,
        };
        // A booked diner's deal is held until the restaurant decides.
        if bill.savings > 0 {
            let claim = DealClaim {
                bill_id: bill.id.clone(),
                restaurant_id: r.detail.id.clone(),
                date: self.p.clock.local_date(&self.city_tz(&r.detail.city_id).await?, 0),
                intent_id: intent.id.clone(),
                claimed_at: iso(now),
                booking_id: bill.booking_id.clone(),
                pending_offline: true,
            };
            let ev = self.event(
                Some(&r.detail.id),
                Some(&user.id),
                DomainEvent::DealClaimed { bill_id: bill.id.clone(), booking_id: bill.booking_id.clone(), intent_id: intent.id.clone(), offline: true },
            );
            match self.p.billing.claim_deal(&claim, r.detail.settings.daily_deal_limit, &self.hold_cutoff(), &[ev]).await? {
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
        }
        let events = vec![
            self.payment_started(&intent).await?,
            self.event(Some(&r.detail.id), Some(&user.id), DomainEvent::OfflinePaymentSubmitted { payment: payment.clone() }),
        ];
        if let Err(e) = self.p.offline.insert_offline(&payment, &intent, &events).await {
            if bill.savings > 0 {
                let ev = self.deal_released(&r.detail.id, Some(&user.id), &bill.id, &intent.id, "The counter payment couldn't be recorded");
                self.p.billing.release_deal(&bill.id, &intent.id, &[ev]).await?;
            }
            return Err(e);
        }
        Ok(payment)
    }

    /// The diner's latest counter payment for a bill (to show its status).
    pub async fn offline_payment_for_bill(&self, user: &User, bill_id: &str) -> AppResult<Option<OfflinePayment>> {
        let p = self.p.offline.latest_for_bill(bill_id).await?;
        Ok(p.filter(|p| p.user_id == user.id || user.can_manage(&p.restaurant.id)))
    }

    /// Owner/staff view: counter payments, waiting ones first.
    pub async fn restaurant_offline_payments(&self, user: &User, restaurant_id: &str, status: Option<OfflinePaymentStatus>) -> AppResult<Vec<OfflinePayment>> {
        self.managed_restaurant(user, restaurant_id).await?;
        let mut list = self.p.offline.offline_for_restaurant(restaurant_id, status, 200).await?;
        list.sort_by(|a, b| {
            (a.status != OfflinePaymentStatus::PendingApproval, std::cmp::Reverse(&a.submitted_at))
                .cmp(&(b.status != OfflinePaymentStatus::PendingApproval, std::cmp::Reverse(&b.submitted_at)))
        });
        Ok(list)
    }

    async fn load_offline(&self, id: &str) -> AppResult<(OfflinePayment, PaymentIntent)> {
        let p = self.p.offline.get_offline(id).await?.ok_or_else(|| AppError::not_found("Payment"))?;
        let intent = self.p.billing.get_intent(&p.intent_id).await?.ok_or_else(|| AppError::not_found("Payment"))?;
        Ok((p, intent))
    }

    fn expires_ms(p: &OfflinePayment) -> i64 {
        chrono::DateTime::parse_from_rfc3339(&p.expires_at).map(|d| d.timestamp_millis()).unwrap_or(i64::MAX)
    }

    /// The restaurant received the money: the bill is paid.
    pub async fn approve_offline_payment(&self, user: &User, id: &str, amount_received: Option<Paise>) -> AppResult<OfflinePayment> {
        let (mut p, mut intent) = self.load_offline(id).await?;
        let r = self.editable_restaurant(user, &p.restaurant.id).await.map_err(|e| if e.code == "NOT_FOUND" { AppError::not_found("Payment") } else { e })?;
        rules::decide(p.status, Decision::Approve, p.amount, amount_received, None, self.p.clock.now_ms(), Self::expires_ms(&p)).map_err(rule_error)?;
        let bill = self.p.billing.get_bill(&p.bill_id).await?.ok_or_else(|| AppError::not_found("Bill"))?;
        let now = self.p.clock.now_iso();
        let bp = BillPayment {
            id: self.p.ids.new_id("bp"),
            user_id: p.user_id.clone(),
            bill: bill.clone(),
            restaurant: r.detail.reference(),
            payment: PaymentRef {
                intent_id: intent.id.clone(),
                method: p.method,
                amount: p.amount,
                currency: p.currency.clone(),
                provider: OFFLINE_PROVIDER.into(),
                paid_at: now.clone(),
                settlement: Settlement::Offline,
            },
        };
        intent.status = PaymentStatus::Succeeded;
        intent.result_ref = Some(bp.id.clone());
        p.status = OfflinePaymentStatus::Approved;
        p.decided_at = Some(now);
        p.decided_by = Some(user.id.clone());
        p.amount_received = amount_received.or(Some(p.amount));
        p.bill_payment_id = Some(bp.id.clone());
        let mut events = self.bill_paid_events(&bill, &bp, &p.user_id);
        events.push(self.event(Some(&p.restaurant.id), Some(&p.user_id), DomainEvent::OfflinePaymentApproved { payment: p.clone() }));
        match self.p.offline.approve_offline(&p, &intent, &bp, &events).await {
            Ok(true) => Ok(p),
            Ok(false) => Err(AppError::conflict("ALREADY_DECIDED", "Someone already decided this payment")),
            Err(e) if e.code == "ALREADY_PAID" => {
                // Paid on Feast meanwhile (e.g. by card): this one can't count too.
                let (fresh, intent) = self.load_offline(id).await?;
                self.close_offline(fresh, intent, OfflinePaymentStatus::Rejected, None, "The bill was already paid on Feast").await?;
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    /// The restaurant didn't receive it (reason shown to the diner).
    pub async fn reject_offline_payment(&self, user: &User, id: &str, reason: &str) -> AppResult<OfflinePayment> {
        let (p, intent) = self.load_offline(id).await?;
        self.editable_restaurant(user, &p.restaurant.id).await.map_err(|e| if e.code == "NOT_FOUND" { AppError::not_found("Payment") } else { e })?;
        rules::decide(p.status, Decision::Reject, p.amount, None, Some(reason), self.p.clock.now_ms(), Self::expires_ms(&p)).map_err(rule_error)?;
        self.close_offline(p, intent, OfflinePaymentStatus::Rejected, Some(&user.id), reason.trim()).await
    }

    async fn close_offline(
        &self,
        mut p: OfflinePayment,
        mut intent: PaymentIntent,
        status: OfflinePaymentStatus,
        decided_by: Option<&str>,
        reason: &str,
    ) -> AppResult<OfflinePayment> {
        p.status = status;
        p.decided_at = Some(self.p.clock.now_iso());
        p.decided_by = decided_by.map(str::to_owned);
        p.reason = Some(reason.chars().take(300).collect());
        intent.status = PaymentStatus::Failed;
        intent.failure_reason = p.reason.clone();
        let rid = p.restaurant.id.clone();
        let mut events = vec![
            self.event(
                Some(&rid),
                Some(&p.user_id),
                match status {
                    OfflinePaymentStatus::Expired => DomainEvent::OfflinePaymentExpired { payment: p.clone() },
                    _ => DomainEvent::OfflinePaymentRejected { payment: p.clone() },
                },
            ),
            self.event(
                Some(&rid),
                Some(&p.user_id),
                DomainEvent::PaymentFailed {
                    intent_id: intent.id.clone(),
                    purpose: intent.purpose,
                    reference_id: intent.reference_id.clone(),
                    amount: intent.amount,
                    currency: intent.currency.clone(),
                    reason: p.reason.clone(),
                },
            ),
        ];
        if p.savings > 0 {
            events.push(self.deal_released(&rid, Some(&p.user_id), &p.bill_id, &intent.id, reason));
        }
        if !self.p.offline.close_offline(&p, &intent, &events).await? {
            return Err(AppError::conflict("ALREADY_DECIDED", "Someone already decided this payment"));
        }
        Ok(p)
    }

    /// Background job: counter payments nobody confirmed in time lapse, so the
    /// diner can pay again and the held deal goes back to the day's budget.
    pub async fn sweep_offline_payments(&self) -> AppResult<usize> {
        actor::scope(feast_domain::audit::Actor::system("sweeper"), async {
            let due = self.p.offline.expired_pending(&self.p.clock.now_iso(), 100).await?;
            let mut n = 0;
            for p in due {
                let Some(intent) = self.p.billing.get_intent(&p.intent_id).await? else { continue };
                let reason = format!("Nobody confirmed it within {} hours", rules::APPROVAL_WINDOW_MINUTES / 60);
                match self.close_offline(p, intent, OfflinePaymentStatus::Expired, None, &reason).await {
                    Ok(_) => n += 1,
                    Err(e) if e.code == "ALREADY_DECIDED" => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(n)
        })
        .await
    }
}
