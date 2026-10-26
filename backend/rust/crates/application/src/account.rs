//! Account deletion (required by the App Store and Google Play).
//! See `feast_domain::account` for what is removed and what is kept.

use feast_domain::account::{self as rules, DeleteAccountRequest, DeleteAccountResult, OwnerBlocker};
use feast_domain::model::*;
use feast_domain::{DomainEvent, Party};

use crate::context::Feast;
use crate::error::{AppError, AppResult, ErrorKind};

impl Feast {
    /// `DELETE /v1/me`: the signed-in diner or owner deletes their account.
    pub async fn delete_account(&self, user: &User, req: &DeleteAccountRequest) -> AppResult<DeleteAccountResult> {
        if user.role == Role::Admin {
            return Err(AppError::new(
                ErrorKind::Forbidden,
                "ADMIN_CANNOT_SELF_DELETE",
                "Feast staff accounts are removed by another admin, not from the app.",
            ));
        }
        if !rules::confirmed(&req.confirm) {
            return Err(AppError::validation("CONFIRM_REQUIRED", format!("Type {} to confirm", rules::CONFIRM_WORD)));
        }
        self.reauthenticate(user, req).await?;

        // Owners: the restaurants they alone run must not leave diners hanging.
        let mut paused = vec![];
        let mut sole = vec![];
        if user.role == Role::Owner {
            let mut blockers = vec![];
            for rid in &user.restaurant_ids {
                let others = self.p.users.owners_of(rid).await?.into_iter().filter(|o| o.id != user.id && o.is_active()).count();
                if others > 0 {
                    continue;
                }
                let Some(r) = self.p.restaurants.get(rid).await? else { continue };
                let pending = self.p.offline.offline_for_restaurant(rid, Some(OfflinePaymentStatus::PendingApproval), 500).await?.len();
                if pending > 0 {
                    blockers.push(OwnerBlocker::PendingCounterPayments { restaurant: r.detail.name.clone(), count: pending });
                }
                let open = self.p.cases.cases_for_restaurant(rid).await?.into_iter().filter(|c| c.status != feast_domain::trust::CaseStatus::Resolved).count();
                if open > 0 {
                    blockers.push(OwnerBlocker::OpenCases { restaurant: r.detail.name.clone(), count: open });
                }
                sole.push(r);
            }
            if !blockers.is_empty() {
                let msg = blockers.iter().map(OwnerBlocker::message).collect::<Vec<_>>().join(" ");
                return Err(AppError::conflict("OWNER_HAS_OPEN_ITEMS", msg));
            }
        } else {
            let waiting = self.p.offline.pending_for_user(&user.id).await?;
            if !waiting.is_empty() {
                return Err(AppError::conflict(
                    "COUNTER_PAYMENT_PENDING",
                    "You have a counter payment waiting for the restaurant to confirm. Try again once it's confirmed or rejected.",
                ));
            }
        }

        // Upcoming bookings: cancel them so the tables go back on sale and the
        // restaurants are told (same event as a normal cancellation).
        let mut cancelled = 0;
        for listed in self.p.bookings.list_by_user(&user.id).await? {
            let tz = self.restaurant_tz(&listed.restaurant.id).await?;
            let today = self.p.clock.local_date(&tz, 0);
            // State-checked: if a payment or a cancellation changes the booking
            // meanwhile, read it again (a just-paid booking is cancelled with
            // its payment, so it is refunded).
            let mut b = listed;
            let mut done = None;
            for _ in 0..3 {
                let eff = self.effective_booking(b.clone(), &tz);
                if !rules::cancels_booking(eff.status, &eff.date, &today) {
                    break;
                }
                let mut updated = b.clone();
                updated.status = BookingStatus::Cancelled;
                let ev = self.event(
                    Some(&updated.restaurant.id),
                    Some(&updated.user_id),
                    DomainEvent::BookingCancelled { booking: updated.clone(), by: Party::Customer },
                );
                if self.p.bookings.update_if_status(&updated, b.status, &[ev]).await? {
                    done = Some(updated);
                    break;
                }
                match self.p.bookings.get(&b.id).await? {
                    Some(fresh) => b = fresh,
                    None => break,
                }
            }
            let Some(updated) = done else { continue };
            if let Err(e) = self.refund_cancelled_booking(&updated, Party::Customer, true).await {
                tracing::error!(booking = %updated.id, error = %e.message, "couldn't start the refund for a booking cancelled by account deletion");
            }
            cancelled += 1;
        }

        for r in sole {
            if r.detail.status == RestaurantStatus::Live {
                self.set_restaurant_status(user, &r.detail.id, RestaurantStatus::Paused).await?;
                paused.push(r.detail.name.clone());
            }
        }

        let at = self.p.clock.now_iso();
        let name = rules::anonymised_name(user.role);
        // Recorded before the personal data goes, as the deleting user.
        self.audit_direct(None, "user", &user.id, "user.account_deleted", |a| {
            a.subject_user_id = Some(user.id.clone());
            a.from_status = Some("ACTIVE".into());
            a.to_status = Some("DELETED".into());
            a.note = Some(format!("Cancelled {cancelled} upcoming booking(s); paused {} restaurant(s)", paused.len()));
        })
        .await?;
        self.p.users.anonymise(&user.id, name, &at).await?;
        tracing::info!(user = %user.id, cancelled, paused = paused.len(), "account deleted");
        Ok(DeleteAccountResult { deleted: true, cancelled_bookings: cancelled, paused_restaurants: paused })
    }

    /// A fresh proof of identity before something irreversible.
    async fn reauthenticate(&self, user: &User, req: &DeleteAccountRequest) -> AppResult<()> {
        match user.role {
            Role::Customer => match &user.phone {
                // Phone sign-in: a fresh code (request it with POST /v1/auth/otp).
                Some(phone) => {
                    let code = req
                        .otp
                        .as_deref()
                        .map(str::trim)
                        .filter(|c| !c.is_empty())
                        .ok_or_else(|| AppError::validation("OTP_REQUIRED", "Enter the code we just sent to your phone"))?;
                    if !self.check_otp(phone, code).await? {
                        return Err(AppError::validation("BAD_OTP", "That code is incorrect"));
                    }
                    Ok(())
                }
                // Social sign-in only: the confirmation word is the check.
                None => Ok(()),
            },
            _ => {
                let password = req.password.as_deref().unwrap_or_default();
                let email = user.email.as_deref().unwrap_or_default();
                let hash = self.p.users.find_by_email(email).await?.and_then(|(_, h)| h);
                match hash {
                    Some(h) if !password.is_empty() && self.p.passwords.verify(password, &h) => Ok(()),
                    _ => Err(AppError::validation("BAD_PASSWORD", "Your password is incorrect")),
                }
            }
        }
    }
}
