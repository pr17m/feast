//! Refunds: money Feast took through its payment provider going back to the
//! diner's card (`feast_domain::refunds`).
//!
//! Every refund is first *recorded* (PENDING, once per reason thanks to its
//! idempotency key, with its events in the same transaction), then *sent* to
//! the provider. If the provider can't be reached the refund stays PENDING and
//! the sweeper sends it again; the refund id is the provider idempotency key,
//! so a retry never pays twice. The provider's webhook (or its answer) moves
//! it to SUCCEEDED or FAILED. Counter payments never went through Feast and
//! are never refunded by Feast.

use feast_domain::model::*;
use feast_domain::refunds::{self as rules, Refund, RefundReason, RefundRequest, RefundStatus};
use feast_domain::{DomainEvent, Party};

use crate::context::Feast;
use crate::error::{AppError, AppResult};
use crate::offline::OFFLINE_PROVIDER;
use crate::ports::RefundOutcome;

/// Provider calls before a refund that can't be sent is given up (FAILED, admins told).
pub const MAX_SEND_ATTEMPTS: i64 = 8;

impl Feast {
    /// Record a refund of (part of) a SUCCEEDED online payment and send it.
    /// Idempotent per `key`: asking again returns the refund already made.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn start_refund(
        &self,
        intent: &PaymentIntent,
        amount: Option<Paise>,
        reason: RefundReason,
        key: String,
        note: Option<String>,
        case_id: Option<String>,
        requested_by: Option<String>,
    ) -> AppResult<Refund> {
        if intent.provider == OFFLINE_PROVIDER {
            return Err(AppError::conflict(
                "OFFLINE_NOT_REFUNDABLE",
                "This was paid at the counter, so it never went through Feast. The restaurant refunds it directly.",
            ));
        }
        if intent.status != PaymentStatus::Succeeded {
            return Err(AppError::conflict("NOT_REFUNDABLE", "Only a completed payment can be refunded"));
        }
        let existing = self.p.refunds.refunds_for_intent(&intent.id).await?;
        if let Some(r) = existing.iter().find(|r| r.idempotency_key == key) {
            return Ok(r.clone());
        }
        let amount = rules::check_amount(amount, rules::refundable(intent.amount, &existing)).map_err(|m| AppError::validation("INVALID_REFUND", m))?;
        let now = self.p.clock.now_iso();
        let refund = Refund {
            id: self.p.ids.new_id("rf"),
            intent_id: intent.id.clone(),
            purpose: intent.purpose,
            reference_id: intent.reference_id.clone(),
            user_id: intent.user_id.clone(),
            restaurant_id: self.restaurant_of(intent).await?,
            amount,
            currency: intent.currency.clone(),
            provider: intent.provider.clone(),
            provider_refund_id: None,
            status: RefundStatus::Pending,
            reason,
            idempotency_key: key,
            note,
            case_id,
            requested_by,
            failure: None,
            attempts: 0,
            created_at: now.clone(),
            updated_at: now,
        };
        let ev = self.refund_event(&refund, |r| DomainEvent::RefundRequested { refund: r });
        let (stored, new) = self.p.refunds.insert_refund(&refund, &[ev]).await?;
        if !new {
            return Ok(stored);
        }
        self.send_refund(stored).await
    }

    fn refund_event(&self, r: &Refund, make: impl FnOnce(Refund) -> DomainEvent) -> feast_domain::EventEnvelope {
        self.event(r.restaurant_id.as_deref(), r.user_id.as_deref(), make(r.clone()))
    }

    /// Ask the provider to send a PENDING, unsent refund. Provider outages
    /// leave it PENDING for the sweeper; never an error for the caller.
    pub(crate) async fn send_refund(&self, mut r: Refund) -> AppResult<Refund> {
        if r.status != RefundStatus::Pending || r.provider_refund_id.is_some() {
            return Ok(r);
        }
        let intent = self.p.billing.get_intent(&r.intent_id).await?.ok_or_else(|| AppError::not_found("Payment"))?;
        r.attempts += 1;
        r.updated_at = self.p.clock.now_iso();
        let outcome = match self.p.payments.get(&r.provider) {
            Some(g) => g.refund(&intent, &r).await,
            None => Err(AppError::new(crate::error::ErrorKind::Unavailable, "PAYMENTS_UNAVAILABLE", "Payment provider not configured")),
        };
        let mut events = vec![];
        match outcome {
            Ok(RefundOutcome { provider_refund_id, status, failure }) => {
                r.provider_refund_id = provider_refund_id;
                r.failure = failure;
                r.status = status;
                match status {
                    RefundStatus::Succeeded => events.push(self.refund_event(&r, |x| DomainEvent::RefundSucceeded { refund: x })),
                    RefundStatus::Failed => events.push(self.refund_event(&r, |x| DomainEvent::RefundFailed { refund: x })),
                    // Accepted; the provider's webhook will say when it's done.
                    RefundStatus::Pending if r.provider_refund_id.is_none() => {
                        r.failure = Some("The provider didn't return a refund id; will retry".into());
                    }
                    RefundStatus::Pending => {}
                }
            }
            Err(e) if r.attempts >= MAX_SEND_ATTEMPTS => {
                tracing::error!(refund = %r.id, error = %e.message, "refund could not be sent; giving up");
                r.status = RefundStatus::Failed;
                r.failure = Some(format!("Couldn't reach the payment provider: {}", e.message));
                events.push(self.refund_event(&r, |x| DomainEvent::RefundFailed { refund: x }));
            }
            Err(e) => {
                tracing::warn!(refund = %r.id, attempt = r.attempts, error = %e.message, "refund not sent yet; the sweeper will retry");
                r.failure = Some(format!("Not sent yet (attempt {}): {}", r.attempts, e.message));
            }
        }
        self.p.refunds.update_refund(&r, &events).await?;
        Ok(self.p.refunds.get_refund(&r.id).await?.unwrap_or(r))
    }

    /// A provider webhook about a refund.
    pub(crate) async fn apply_refund_webhook(
        &self,
        provider: &str,
        provider_refund_id: &str,
        status: RefundStatus,
        failure: Option<String>,
    ) -> AppResult<bool> {
        let Some(mut r) = self.p.refunds.find_refund_by_provider_id(provider, provider_refund_id).await? else {
            tracing::warn!(provider, provider_refund_id, "webhook for an unknown refund; ignoring");
            return Ok(false);
        };
        if !rules::may_move(r.status, status) {
            return Ok(false); // late or repeated: a final refund stays final
        }
        r.status = status;
        r.failure = failure;
        r.updated_at = self.p.clock.now_iso();
        let ev = match status {
            RefundStatus::Succeeded => self.refund_event(&r, |x| DomainEvent::RefundSucceeded { refund: x }),
            _ => self.refund_event(&r, |x| DomainEvent::RefundFailed { refund: x }),
        };
        self.p.refunds.update_refund(&r, &[ev]).await
    }

    /// Sweeper: send refunds the provider hasn't accepted yet.
    pub async fn sweep_refunds(&self) -> AppResult<usize> {
        let mut sent = 0;
        for r in self.p.refunds.unsent_refunds(50).await? {
            let after = self.send_refund(r).await?;
            if after.provider_refund_id.is_some() || after.status != RefundStatus::Pending {
                sent += 1;
            }
        }
        Ok(sent)
    }

    /// The upfront charge of a cancelled booking goes back when the rules say
    /// so (restaurant/Feast cancelled, or the diner cancelled in time). Never
    /// fails the cancellation: a refund that can't be sent is retried.
    pub(crate) async fn refund_cancelled_booking(&self, b: &Booking, by: Party, account_deleted: bool) -> AppResult<Option<Refund>> {
        let Some(paid) = b.payment.as_ref().filter(|p| p.settlement == Settlement::Online && p.amount > 0) else { return Ok(None) };
        let tz = self.restaurant_tz(&b.restaurant.id).await?;
        let slot = self.p.clock.local_to_ms(&tz, &b.date, &b.time);
        let Some(mut reason) = rules::booking_cancel_refund(by, slot, self.p.clock.now_ms()) else {
            tracing::info!(booking = %b.id, "cancelled late: the restaurant keeps the upfront charge");
            return Ok(None);
        };
        if account_deleted && reason == RefundReason::CancelledInTime {
            reason = RefundReason::AccountDeleted;
        }
        let Some(intent) = self.p.billing.get_intent(&paid.intent_id).await? else { return Ok(None) };
        Ok(Some(self.start_refund(&intent, None, reason, format!("booking-cancel:{}", b.id), None, None, None).await?))
    }

    /// A payment that succeeded at the provider but can't be applied (booking
    /// cancelled/expired or its table gone; bill already paid): mark it
    /// settled so it can never be applied later, and refund it in full.
    pub(crate) async fn payment_not_applied(&self, mut intent: PaymentIntent, reason: RefundReason, why: &str) -> AppResult<PaymentIntent> {
        intent.status = PaymentStatus::Succeeded;
        intent.failure_reason = Some(format!("{why} The payment is being refunded."));
        let now = self.p.clock.now_iso();
        let refund = Refund {
            id: self.p.ids.new_id("rf"),
            intent_id: intent.id.clone(),
            purpose: intent.purpose,
            reference_id: intent.reference_id.clone(),
            user_id: intent.user_id.clone(),
            restaurant_id: self.restaurant_of(&intent).await?,
            amount: intent.amount,
            currency: intent.currency.clone(),
            provider: intent.provider.clone(),
            provider_refund_id: None,
            status: RefundStatus::Pending,
            reason,
            idempotency_key: format!("unapplied:{}", intent.id),
            note: Some(why.to_owned()),
            case_id: None,
            requested_by: None,
            failure: None,
            attempts: 0,
            created_at: now.clone(),
            updated_at: now,
        };
        let events = vec![
            self.event(
                refund.restaurant_id.as_deref(),
                intent.user_id.as_deref(),
                DomainEvent::PaymentNotApplied {
                    intent_id: intent.id.clone(),
                    purpose: intent.purpose,
                    reference_id: intent.reference_id.clone(),
                    amount: intent.amount,
                    currency: intent.currency.clone(),
                    reason: why.to_owned(),
                },
            ),
            self.refund_event(&refund, |r| DomainEvent::RefundRequested { refund: r }),
        ];
        if self.p.billing.record_unapplied_payment(&intent, &refund, &events).await? {
            tracing::warn!(intent = %intent.id, why, "payment arrived but couldn't be applied; refunding it");
            self.send_refund(refund).await?;
        }
        self.p.billing.get_intent(&intent.id).await?.ok_or_else(|| AppError::not_found("Payment"))
    }

    /// Refunds of one payment: for the diner who paid, the restaurant's
    /// managers and admins.
    pub async fn payment_refunds(&self, user: &User, intent_id: &str) -> AppResult<Vec<Refund>> {
        let intent = self.p.billing.get_intent(intent_id).await?.ok_or_else(|| AppError::not_found("Payment"))?;
        let mine = intent.user_id.as_deref() == Some(user.id.as_str());
        let managed = match self.restaurant_of(&intent).await? {
            Some(rid) => user.can_manage(&rid),
            None => user.role == Role::Admin,
        };
        if !mine && !managed {
            return Err(AppError::not_found("Payment"));
        }
        self.p.refunds.refunds_for_intent(intent_id).await
    }

    /// Admin: refund (part of) an online payment by hand (amount ≤ what is left).
    pub async fn admin_refund(&self, user: &User, req: RefundRequest) -> AppResult<Refund> {
        self.require_role(user, &[Role::Admin])?;
        let note = req.note.trim();
        if note.chars().count() < rules::MIN_NOTE_CHARS {
            return Err(AppError::validation("NOTE_REQUIRED", format!("Say why in at least {} characters", rules::MIN_NOTE_CHARS)));
        }
        let intent = self.p.billing.get_intent(&req.intent_id).await?.ok_or_else(|| AppError::not_found("Payment"))?;
        let reason = if req.case_id.is_some() { RefundReason::CaseDecision } else { RefundReason::Manual };
        // Each manual refund is its own: a fresh key (the amount check stops over-refunding).
        let key = format!("manual:{}", self.p.ids.new_id("rk"));
        self.start_refund(&intent, req.amount, reason, key, Some(note.to_owned()), req.case_id.clone(), Some(user.id.clone())).await
    }

    /// Admin: latest refunds (newest first).
    pub async fn admin_refunds(&self, user: &User) -> AppResult<Vec<Refund>> {
        self.require_role(user, &[Role::Admin])?;
        self.p.refunds.list_refunds(200).await
    }
}
