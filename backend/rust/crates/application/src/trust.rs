//! Trust & safety use cases: who may edit a restaurant's content, Feast's
//! moderation, owner-granted support access (audited), grievance cases between
//! diners and restaurants, and verified reviews. Rules live in
//! `feast_domain::trust`; this module loads, checks access, applies and stores.

use chrono::Duration;
use feast_domain::model::*;
use feast_domain::trust::{self as rules, CaseRuleError};
use feast_domain::DomainEvent;

use crate::compliance::DocumentFile;
use crate::context::Feast;
use crate::error::{AppError, AppResult, ErrorKind};

/// A photo attached to a case, as parsed by the delivery layer.
#[derive(Debug, Clone)]
pub struct CaseUpload {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

const IMAGE_TYPES: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];
const MAX_CASE_FILE_BYTES: usize = 5 * 1_048_576;

fn rule(e: CaseRuleError) -> AppError {
    let kind = if e.code == "FORBIDDEN" { ErrorKind::Forbidden } else { ErrorKind::Conflict };
    AppError::new(kind, e.code, e.message)
}

fn changed() -> AppError {
    AppError::conflict("CASE_CHANGED", "This case just changed — refresh and try again")
}

fn clean(text: &str, max: usize) -> String {
    text.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(max).collect()
}

fn require_text(text: &str, min: usize, max: usize, what: &str) -> AppResult<String> {
    let t = clean(text, max + 1);
    if t.chars().count() < min {
        return Err(AppError::validation("TEXT_TOO_SHORT", format!("{what} needs at least {min} characters")));
    }
    if t.chars().count() > max {
        return Err(AppError::validation("TEXT_TOO_LONG", format!("{what} can be at most {max} characters")));
    }
    Ok(t)
}

impl Feast {
    // ============================================================ content access

    /// A restaurant the caller may *change*: its owner, or Feast staff while
    /// the owner has granted support access (or before any owner is linked,
    /// e.g. a restaurant an admin is setting up). Viewing uses `managed_restaurant`.
    pub(crate) async fn editable_restaurant(&self, user: &User, restaurant_id: &str) -> AppResult<Restaurant> {
        let r = self.managed_restaurant(user, restaurant_id).await?;
        if user.role == Role::Admin && !self.staff_may_edit(&r).await? {
            return Err(AppError::new(
                ErrorKind::Forbidden,
                "SUPPORT_ACCESS_REQUIRED",
                "Only the restaurant's owner can change this. Ask them to grant Feast support access in Settings.",
            ));
        }
        Ok(r)
    }

    pub(crate) async fn staff_may_edit(&self, r: &Restaurant) -> AppResult<bool> {
        Ok(rules::support_active(r.support_access_until.as_deref(), self.now()) || self.p.users.owners_of(&r.detail.id).await?.is_empty())
    }

    /// Record an action in the restaurant's audit log.
    pub(crate) async fn record_audit(&self, user: &User, restaurant_id: &str, action: &str, summary: String) -> AppResult<()> {
        self.p
            .audit
            .record(&AuditEntry {
                id: self.p.ids.new_id("aud"),
                restaurant_id: restaurant_id.to_owned(),
                actor_id: user.id.clone(),
                actor_name: user.name.clone(),
                actor_role: user.role,
                action: action.to_owned(),
                summary: summary.chars().take(300).collect(),
                at: self.p.clock.now_iso(),
            })
            .await
    }

    /// Audit a content/settings edit: always in the audit trail; Feast staff
    /// edits also in the support-access log the owner reviews.
    pub(crate) async fn audit_staff_edit(&self, user: &User, restaurant_id: &str, action: &str, summary: String) -> AppResult<()> {
        if user.role == Role::Admin {
            self.record_audit(user, restaurant_id, action, summary.clone()).await?;
        }
        let entity = action.split('.').next().unwrap_or("restaurant");
        self.audit_direct(Some(restaurant_id), entity, restaurant_id, action, |r| r.note = Some(summary.chars().take(300).collect())).await
    }

    /// Append an audit record for a change that has no domain event. The
    /// current actor (see `crate::actor`) is recorded as who did it.
    pub(crate) async fn audit_direct(
        &self,
        restaurant_id: Option<&str>,
        entity_type: &str,
        entity_id: &str,
        action: &str,
        fill: impl FnOnce(&mut feast_domain::audit::AuditRecord),
    ) -> AppResult<()> {
        let mut r =
            feast_domain::audit::AuditRecord::direct(self.p.ids.new_id("aud"), self.p.clock.now_iso(), crate::actor::current(), entity_type, entity_id, action);
        r.restaurant_id = restaurant_id.map(str::to_owned);
        fill(&mut r);
        self.p.trail.append(&r).await
    }

    // ============================================================ support access

    pub async fn support_access(&self, user: &User, restaurant_id: &str) -> AppResult<SupportAccess> {
        let r = self.managed_restaurant(user, restaurant_id).await?;
        let staff_can_edit = self.staff_may_edit(&r).await?;
        let active_until = r.support_access_until.filter(|u| rules::support_active(Some(u), self.now()));
        Ok(SupportAccess { active_until, staff_can_edit, audit: self.p.audit.audit_for(restaurant_id, 100).await? })
    }

    /// Owner grants Feast staff edit access for `hours` (1–72), or revokes it (0).
    pub async fn set_support_access(&self, user: &User, restaurant_id: &str, hours: i64) -> AppResult<SupportAccess> {
        if !user.owns(restaurant_id) {
            return Err(AppError::forbidden("Only the restaurant's owner can grant support access"));
        }
        if !(0..=rules::MAX_SUPPORT_HOURS).contains(&hours) {
            return Err(AppError::validation("INVALID_HOURS", format!("Choose between 1 and {} hours", rules::MAX_SUPPORT_HOURS)));
        }
        let mut r = self.managed_restaurant(user, restaurant_id).await?;
        let until = (hours > 0).then(|| rules::iso(self.now() + Duration::hours(hours)));
        r.support_access_until = until.clone();
        self.p.restaurants.update(&r).await?;
        let summary = match &until {
            Some(u) => format!("Granted Feast support access for {hours} h (until {u})"),
            None => "Revoked Feast support access".into(),
        };
        self.record_audit(user, restaurant_id, if hours > 0 { "support_access.granted" } else { "support_access.revoked" }, summary).await?;
        let ev = self.event(Some(restaurant_id), None, DomainEvent::SupportAccessChanged { restaurant: r.detail.reference(), until });
        self.p.outbox.append(&[ev]).await?;
        self.support_access(user, restaurant_id).await
    }

    // ============================================================ moderation

    /// Feast hides (with a reason the owner sees) or restores one menu item / offer.
    pub async fn moderate_content(&self, user: &User, restaurant_id: &str, req: ModerateContentRequest) -> AppResult<()> {
        self.require_role(user, &[Role::Admin])?;
        let mut r = self.p.restaurants.get(restaurant_id).await?.ok_or_else(|| AppError::not_found("Restaurant"))?;
        let reason = match req.action {
            ModerationAction::Hide => Some(require_text(req.reason.as_deref().unwrap_or_default(), rules::MIN_REASON_CHARS, 300, "The reason")?),
            ModerationAction::Restore => None,
        };
        let new = reason.clone().map(|reason| Moderation {
            reason,
            hidden_at: self.p.clock.now_iso(),
            hidden_by: user.id.clone(),
            review_requested_at: None,
            review_note: None,
        });
        let (title, slot) = match req.kind {
            ContentKind::MenuItem => {
                let i = r.find_item_mut(&req.id).ok_or_else(|| AppError::not_found("Menu item"))?;
                (i.name.clone(), &mut i.moderation)
            }
            ContentKind::Offer => {
                let o = r.detail.offers.iter_mut().find(|o| o.id == req.id).ok_or_else(|| AppError::not_found("Offer"))?;
                (o.title.clone(), &mut o.moderation)
            }
        };
        if slot.is_none() && new.is_none() {
            return Ok(()); // already visible
        }
        *slot = new;
        self.p.restaurants.update(&r).await?;
        let hidden = req.action == ModerationAction::Hide;
        let what = if req.kind == ContentKind::MenuItem { "menu item" } else { "offer" };
        self.record_audit(
            user,
            restaurant_id,
            if hidden { "moderation.hidden" } else { "moderation.restored" },
            match &reason {
                Some(why) => format!("Hid {what} “{title}”: {why}"),
                None => format!("Restored {what} “{title}”"),
            },
        )
        .await?;
        let ev =
            self.event(Some(restaurant_id), None, DomainEvent::ContentModerated { restaurant: r.detail.reference(), kind: req.kind, title, hidden, reason });
        self.p.outbox.append(&[ev]).await
    }

    /// Hidden items/offers across restaurants; owners' appeals first.
    pub async fn moderation_queue(&self, user: &User) -> AppResult<Vec<ModeratedContent>> {
        self.require_role(user, &[Role::Admin])?;
        let mut out = vec![];
        for r in self.p.restaurants.list().await? {
            let rf = r.detail.reference();
            for i in r.menu.iter().flat_map(|c| c.items.iter()) {
                if let Some(m) = &i.moderation {
                    out.push(ModeratedContent {
                        restaurant: rf.clone(),
                        kind: ContentKind::MenuItem,
                        id: i.id.clone(),
                        title: i.name.clone(),
                        moderation: m.clone(),
                    });
                }
            }
            for o in &r.detail.offers {
                if let Some(m) = &o.moderation {
                    out.push(ModeratedContent {
                        restaurant: rf.clone(),
                        kind: ContentKind::Offer,
                        id: o.id.clone(),
                        title: o.title.clone(),
                        moderation: m.clone(),
                    });
                }
            }
        }
        out.sort_by(|a, b| {
            (b.moderation.review_requested_at.is_some(), &b.moderation.review_requested_at, &b.moderation.hidden_at).cmp(&(
                a.moderation.review_requested_at.is_some(),
                &a.moderation.review_requested_at,
                &a.moderation.hidden_at,
            ))
        });
        Ok(out)
    }

    /// The owner fixed hidden content and asks Feast to look again.
    pub async fn appeal_moderation(&self, user: &User, restaurant_id: &str, req: ModerationAppealRequest) -> AppResult<()> {
        if !user.owns(restaurant_id) {
            return Err(AppError::forbidden("Only the restaurant's owner can ask for a review"));
        }
        let mut r = self.managed_restaurant(user, restaurant_id).await?;
        let note = req.note.as_deref().map(|n| clean(n, 500)).filter(|n| !n.is_empty());
        let now = self.p.clock.now_iso();
        let (title, slot) = match req.kind {
            ContentKind::MenuItem => {
                let i = r.find_item_mut(&req.id).ok_or_else(|| AppError::not_found("Menu item"))?;
                (i.name.clone(), &mut i.moderation)
            }
            ContentKind::Offer => {
                let o = r.detail.offers.iter_mut().find(|o| o.id == req.id).ok_or_else(|| AppError::not_found("Offer"))?;
                (o.title.clone(), &mut o.moderation)
            }
        };
        let Some(m) = slot.as_mut() else {
            return Err(AppError::conflict("NOT_HIDDEN", "This isn't hidden"));
        };
        m.review_requested_at = Some(now);
        m.review_note = note.clone();
        self.p.restaurants.update(&r).await?;
        let ev = self.event(Some(restaurant_id), None, DomainEvent::ModerationAppealed { restaurant: r.detail.reference(), kind: req.kind, title, note });
        self.p.outbox.append(&[ev]).await
    }

    // ============================================================ cases

    fn case_role(&self, user: &User, c: &Case) -> Option<CaseRole> {
        if user.id == c.customer.id {
            Some(CaseRole::Customer)
        } else if user.owns(&c.restaurant.id) {
            Some(CaseRole::Restaurant)
        } else if user.role == Role::Admin {
            Some(CaseRole::Feast)
        } else {
            None
        }
    }

    async fn readable_case(&self, user: &User, id: &str) -> AppResult<(Case, CaseRole)> {
        let c = self.p.cases.get_case(id).await?.ok_or_else(|| AppError::not_found("Case"))?;
        let role = self.case_role(user, &c).ok_or_else(|| AppError::not_found("Case"))?;
        Ok((c, role))
    }

    fn view_for(role: CaseRole, c: Case) -> Case {
        if role == CaseRole::Feast {
            c
        } else {
            c.for_party()
        }
    }

    fn author_name(role: CaseRole, c: &Case, user: &User) -> String {
        match role {
            CaseRole::Customer => rules::short_name(&c.customer.name),
            CaseRole::Restaurant => c.restaurant.name.clone(),
            CaseRole::Feast => format!("{} · Feast", rules::short_name(&user.name)),
        }
    }

    fn message(&self, author: CaseRole, author_name: String, text: String, internal: bool, system: bool) -> CaseMessage {
        CaseMessage { id: self.p.ids.new_id("msg"), author, author_name, text, at: self.p.clock.now_iso(), internal, system }
    }

    /// A diner complains about a visit, or a restaurant reports a diner.
    pub async fn open_case(&self, user: &User, req: OpenCaseRequest) -> AppResult<Case> {
        let kind = match user.role {
            Role::Customer => CaseKind::Complaint,
            Role::Owner => CaseKind::Report,
            Role::Admin => return Err(AppError::forbidden("Feast staff act on cases from the resolution desk")),
        };
        if !req.reason.allowed_for(kind) {
            return Err(AppError::validation("INVALID_REASON", "Choose a reason that fits"));
        }
        let text = require_text(&req.text, rules::MIN_MESSAGE_CHARS, rules::MAX_MESSAGE_CHARS, "Your description")?;
        // The visit: exactly one of a booking or a bill paid on Feast.
        let (restaurant, customer_id, customer_fallback, visit_iso, is_booking_date, amount, currency, booking_id, payment_id) =
            match (&req.booking_id, &req.bill_payment_id) {
                (Some(bid), None) => {
                    let b = self.p.bookings.get(bid).await?.ok_or_else(|| AppError::not_found("Booking"))?;
                    let mine = match kind {
                        CaseKind::Complaint => b.user_id == user.id,
                        CaseKind::Report => user.owns(&b.restaurant.id),
                    };
                    if !mine || b.status == BookingStatus::PendingPayment {
                        return Err(AppError::not_found("Booking"));
                    }
                    let amount = (b.cover_charge > 0).then_some(b.cover_charge);
                    (b.restaurant, b.user_id, b.contact.name, b.date, true, amount, b.currency, Some(b.id), None)
                }
                (None, Some(pid)) => {
                    let p = self.p.billing.get_bill_payment(pid).await?.ok_or_else(|| AppError::not_found("Payment"))?;
                    let mine = match kind {
                        CaseKind::Complaint => p.user_id == user.id,
                        CaseKind::Report => user.owns(&p.restaurant.id),
                    };
                    if !mine {
                        return Err(AppError::not_found("Payment"));
                    }
                    (p.restaurant, p.user_id, String::new(), p.payment.paid_at, false, Some(p.payment.amount), p.payment.currency, None, Some(p.id))
                }
                _ => return Err(AppError::validation("VISIT_REQUIRED", "Choose the booking or the bill this is about")),
            };
        let tz = match self.p.restaurants.get(&restaurant.id).await? {
            Some(r) => self.city_tz(&r.detail.city_id).await?,
            None => String::new(),
        };
        let today = self.p.clock.local_date(&tz, 0);
        let visit_date = if is_booking_date { visit_iso } else { self.p.clock.local_date_of(&tz, &visit_iso).unwrap_or_else(|| today.clone()) };
        let recent = rules::within_days(&visit_date, &today, rules::FILE_WITHIN_DAYS);
        let ok_date = match kind {
            // A diner may complain about a booking the restaurant cancelled ahead of time.
            CaseKind::Complaint => recent || visit_date > today,
            CaseKind::Report => recent,
        };
        if !ok_date {
            return Err(AppError::validation(
                "OUTSIDE_WINDOW",
                match kind {
                    CaseKind::Complaint => format!("Problems must be raised within {} days of the visit", rules::FILE_WITHIN_DAYS),
                    CaseKind::Report => format!("You can report a diner on or after the visit, within {} days", rules::FILE_WITHIN_DAYS),
                },
            ));
        }
        if let Some(existing) = self.p.cases.open_case_for_visit(kind, booking_id.as_deref(), payment_id.as_deref()).await? {
            return Err(AppError::conflict("CASE_EXISTS", format!("There's already an open case about this visit ({})", existing.number)));
        }
        let customer_name = match self.p.users.get(&customer_id).await? {
            Some(u) if !u.name.trim().is_empty() => u.name,
            _ if !customer_fallback.trim().is_empty() => customer_fallback,
            _ => "Feast diner".into(),
        };
        let now = self.now();
        let (status, due) = rules::opening(kind, now);
        let mut c = Case {
            id: self.p.ids.new_id("case"),
            number: format!("FC-{}", self.p.ids.short_code(6).to_uppercase()),
            kind,
            status,
            reason: req.reason,
            restaurant,
            customer: CaseParty { id: customer_id, name: customer_name },
            booking_id,
            bill_payment_id: payment_id,
            visit_date,
            amount,
            currency,
            messages: vec![],
            attachments: vec![],
            outcome: None,
            due_at: Some(due),
            escalated_at: (kind == CaseKind::Report).then(|| rules::iso(now)),
            resolved_at: None,
            created_at: rules::iso(now),
            updated_at: rules::iso(now),
            version: 1,
        };
        let role = if kind == CaseKind::Complaint { CaseRole::Customer } else { CaseRole::Restaurant };
        let author = Self::author_name(role, &c, user);
        c.messages.push(self.message(role, author, text, false, false));
        let ev = self.event(Some(&c.restaurant.id), Some(&c.customer.id), DomainEvent::CaseOpened { case: c.reference() });
        self.p.cases.insert_case(&c, &[ev]).await?;
        Ok(c.for_party())
    }

    /// Cases about the caller as a diner (their complaints and reports about them).
    pub async fn my_cases(&self, user: &User) -> AppResult<Vec<Case>> {
        Ok(self.p.cases.cases_for_customer(&user.id).await?.into_iter().map(Case::for_party).collect())
    }

    pub async fn restaurant_cases(&self, user: &User, restaurant_id: &str) -> AppResult<Vec<Case>> {
        self.managed_restaurant(user, restaurant_id).await?;
        let staff = user.role == Role::Admin;
        Ok(self.p.cases.cases_for_restaurant(restaurant_id).await?.into_iter().map(|c| if staff { c } else { c.for_party() }).collect())
    }

    pub async fn get_case(&self, user: &User, id: &str) -> AppResult<Case> {
        let (c, role) = self.readable_case(user, id).await?;
        Ok(Self::view_for(role, c))
    }

    async fn save_case(&self, c: &mut Case, events: Vec<DomainEvent>) -> AppResult<()> {
        c.version += 1;
        c.updated_at = self.p.clock.now_iso();
        let evs: Vec<_> = events.into_iter().map(|e| self.event(Some(&c.restaurant.id), Some(&c.customer.id), e)).collect();
        if !self.p.cases.update_case(c, &evs).await? {
            return Err(changed());
        }
        Ok(())
    }

    pub async fn case_message(&self, user: &User, id: &str, req: CaseMessageRequest) -> AppResult<Case> {
        let (mut c, role) = self.readable_case(user, id).await?;
        let internal = req.internal.unwrap_or(false);
        let text = require_text(&req.text, 2, rules::MAX_MESSAGE_CHARS, "Your message")?;
        let (status, due) = rules::after_message(&c, role, internal, self.now()).map_err(rule)?;
        let author = Self::author_name(role, &c, user);
        c.messages.push(self.message(role, author, text, internal, false));
        c.status = status;
        c.due_at = due;
        let events = if internal { vec![] } else { vec![DomainEvent::CaseUpdated { case: c.reference(), change: CaseChange::Message, by: role }] };
        self.save_case(&mut c, events).await?;
        Ok(Self::view_for(role, c))
    }

    /// The diner or the restaurant asks Feast to step in.
    pub async fn escalate_case(&self, user: &User, id: &str) -> AppResult<Case> {
        let (mut c, role) = self.readable_case(user, id).await?;
        let now = self.now();
        let (status, due) = rules::escalate(&c, role, now).map_err(rule)?;
        let who = Self::author_name(role, &c, user);
        c.messages.push(self.message(role, who.clone(), format!("{who} asked Feast to step in."), false, true));
        c.status = status;
        c.due_at = Some(due);
        c.escalated_at = Some(rules::iso(now));
        let ev = DomainEvent::CaseUpdated { case: c.reference(), change: CaseChange::Escalated, by: role };
        self.save_case(&mut c, vec![ev]).await?;
        Ok(Self::view_for(role, c))
    }

    /// The diner says their complaint is sorted.
    pub async fn settle_case(&self, user: &User, id: &str) -> AppResult<Case> {
        let (mut c, role) = self.readable_case(user, id).await?;
        if role != CaseRole::Customer {
            return Err(AppError::forbidden("Only the diner who raised this can mark it resolved"));
        }
        rules::settle_by_customer(&c).map_err(rule)?;
        let now = self.p.clock.now_iso();
        c.status = CaseStatus::Resolved;
        c.due_at = None;
        c.resolved_at = Some(now.clone());
        c.outcome = Some(CaseOutcome {
            decision: CaseDecision::SettledBetweenParties,
            note: "The diner marked this as resolved.".into(),
            refund_amount: None,
            decided_by: CaseRole::Customer,
            at: now,
        });
        let ev = DomainEvent::CaseUpdated { case: c.reference(), change: CaseChange::Resolved, by: role };
        self.save_case(&mut c, vec![ev]).await?;
        Ok(c.for_party())
    }

    /// Resolution desk: every case, open and overdue first, with each party's history.
    pub async fn admin_cases(&self, user: &User) -> AppResult<Vec<AdminCaseItem>> {
        self.require_role(user, &[Role::Admin])?;
        let all = self.p.cases.all_cases().await?;
        let now = self.now();
        let count = |f: &dyn Fn(&Case) -> bool| all.iter().filter(|c| f(c)).count() as i64;
        let mut out: Vec<AdminCaseItem> = all
            .iter()
            .map(|c| AdminCaseItem {
                overdue: c.overdue(now),
                restaurant_complaints: count(&|x| x.kind == CaseKind::Complaint && x.restaurant.id == c.restaurant.id),
                customer_reports: count(&|x| x.kind == CaseKind::Report && x.customer.id == c.customer.id),
                case: c.clone(),
            })
            .collect();
        let rank = |i: &AdminCaseItem| match i.case.status {
            CaseStatus::WithFeast => 0,
            CaseStatus::AwaitingRestaurant | CaseStatus::AwaitingCustomer => 1,
            CaseStatus::Resolved => 2,
        };
        out.sort_by(|a, b| {
            (rank(a), !a.overdue, a.case.due_at.clone().unwrap_or_default())
                .cmp(&(rank(b), !b.overdue, b.case.due_at.clone().unwrap_or_default()))
                .then_with(|| b.case.created_at.cmp(&a.case.created_at))
        });
        Ok(out)
    }

    /// Feast decides. Suspending a restaurant or blocking a diner takes effect immediately.
    pub async fn resolve_case(&self, user: &User, id: &str, req: ResolveCaseRequest) -> AppResult<Case> {
        self.require_role(user, &[Role::Admin])?;
        let mut c = self.p.cases.get_case(id).await?.ok_or_else(|| AppError::not_found("Case"))?;
        if c.status == CaseStatus::Resolved {
            return Err(AppError::conflict("CASE_RESOLVED", "This case is already resolved"));
        }
        if !rules::decision_allowed(c.kind, req.decision) {
            return Err(AppError::validation("INVALID_DECISION", "That decision doesn't apply to this kind of case"));
        }
        let note = require_text(&req.note, rules::MIN_REASON_CHARS, 1000, "The decision note")?;
        let refund = match (req.decision, req.refund_amount) {
            (CaseDecision::RefundAdvised, Some(a)) if a > 0 && c.amount.is_none_or(|max| a <= max) => Some(a),
            (CaseDecision::RefundAdvised, Some(_)) => return Err(AppError::validation("INVALID_REFUND", "The refund can't be more than the amount paid")),
            (CaseDecision::RefundAdvised, None) => c.amount,
            _ => None,
        };
        match req.decision {
            CaseDecision::RestaurantSuspended => {
                self.set_restaurant_status(user, &c.restaurant.id, RestaurantStatus::Suspended).await?;
                self.record_audit(user, &c.restaurant.id, "case.restaurant_suspended", format!("Suspended after case {}: {note}", c.number)).await?;
            }
            CaseDecision::CustomerBlocked => {
                if let Some(mut u) = self.p.users.get(&c.customer.id).await? {
                    u.status = UserStatus::Blocked;
                    self.p.users.update(&u).await?;
                }
            }
            CaseDecision::RestaurantWarned => {
                self.record_audit(user, &c.restaurant.id, "case.restaurant_warned", format!("Warned after case {}: {note}", c.number)).await?;
            }
            _ => {}
        }
        // A refund Feast decides is actually sent when the money went through
        // Feast; a counter payment is refunded by the restaurant itself.
        let paid_intent = match refund {
            Some(_) => self.case_payment_intent(&c).await?,
            None => None,
        };
        let how = match (&refund, &paid_intent) {
            (Some(_), Some(i)) if i.provider != crate::offline::OFFLINE_PROVIDER => " Feast is refunding it to the diner's original payment method.",
            (Some(_), Some(_)) => " This was paid at the counter, so the restaurant refunds the diner directly.",
            (Some(_), None) => " There's no Feast payment to refund automatically; the restaurant refunds the diner directly.",
            _ => "",
        };
        if let (Some(a), Some(i)) = (refund, &paid_intent) {
            if i.provider != crate::offline::OFFLINE_PROVIDER {
                // Validate before deciding, so a refund that can't happen doesn't close the case.
                let left = feast_domain::refunds::refundable(i.amount, &self.p.refunds.refunds_for_intent(&i.id).await?);
                feast_domain::refunds::check_amount(Some(a), left).map_err(|m| AppError::validation("INVALID_REFUND", m))?;
            }
        }
        let now = self.p.clock.now_iso();
        let text = match refund {
            Some(a) => format!("Decision: {} ({}).{how} {note}", req.decision.label(), crate::notifications::fmt_money(a, &c.currency)),
            None => format!("Decision: {}. {note}", req.decision.label()),
        };
        c.messages.push(self.message(CaseRole::Feast, Self::author_name(CaseRole::Feast, &c, user), text, false, false));
        c.status = CaseStatus::Resolved;
        c.due_at = None;
        c.resolved_at = Some(now.clone());
        c.outcome = Some(CaseOutcome { decision: req.decision, note, refund_amount: refund, decided_by: CaseRole::Feast, at: now });
        let ev = DomainEvent::CaseUpdated { case: c.reference(), change: CaseChange::Resolved, by: CaseRole::Feast };
        self.save_case(&mut c, vec![ev]).await?;
        if let (Some(a), Some(i)) = (refund, paid_intent.filter(|i| i.provider != crate::offline::OFFLINE_PROVIDER)) {
            let note = c.outcome.as_ref().map(|o| o.note.clone());
            if let Err(e) = self
                .start_refund(
                    &i,
                    Some(a),
                    feast_domain::refunds::RefundReason::CaseDecision,
                    format!("case:{}", c.id),
                    note,
                    Some(c.id.clone()),
                    Some(user.id.clone()),
                )
                .await
            {
                tracing::error!(case = %c.id, error = %e.message, "couldn't start the refund Feast decided; refund it from the admin refunds page");
            }
        }
        Ok(c)
    }

    /// The Feast payment a case is about (its booking's upfront charge, or its bill payment).
    async fn case_payment_intent(&self, c: &Case) -> AppResult<Option<PaymentIntent>> {
        let intent_id = if let Some(bp) = &c.bill_payment_id {
            self.p.billing.get_bill_payment(bp).await?.map(|p| p.payment.intent_id)
        } else if let Some(b) = &c.booking_id {
            self.p.bookings.get(b).await?.and_then(|b| b.payment).map(|p| p.intent_id)
        } else {
            None
        };
        match intent_id {
            Some(id) => self.p.billing.get_intent(&id).await,
            None => Ok(None),
        }
    }

    /// A photo (JPG/PNG/WebP) supporting the case.
    pub async fn add_case_attachment(&self, user: &User, id: &str, up: CaseUpload) -> AppResult<Case> {
        let (mut c, role) = self.readable_case(user, id).await?;
        if !c.status.is_open() {
            return Err(AppError::conflict("CASE_RESOLVED", "This case is resolved"));
        }
        if c.attachments.len() >= rules::MAX_CASE_ATTACHMENTS {
            return Err(AppError::validation("TOO_MANY_FILES", format!("Up to {} photos per case", rules::MAX_CASE_ATTACHMENTS)));
        }
        if up.bytes.is_empty() || up.bytes.len() > MAX_CASE_FILE_BYTES.min(self.settings.max_upload_bytes) {
            return Err(AppError::validation("FILE_TOO_LARGE", "Photos must be under 5 MB"));
        }
        let Some(content_type) = crate::compliance::sniff(&up.bytes).filter(|t| IMAGE_TYPES.contains(t)) else {
            return Err(AppError::validation("INVALID_FILE_TYPE", "Attach a JPG, PNG or WebP photo"));
        };
        let att_id = self.p.ids.new_id("att");
        self.p.files.put(&format!("cases/{}/{att_id}", c.id), up.bytes.clone(), content_type).await?;
        let file_name: String = up.file_name.chars().filter(|ch| !ch.is_control() && *ch != '/' && *ch != '\\').take(120).collect();
        c.attachments.push(CaseAttachment {
            id: att_id,
            file_name: if file_name.is_empty() { "photo".into() } else { file_name },
            content_type: content_type.to_owned(),
            size: up.bytes.len() as i64,
            uploaded_by: role,
            at: self.p.clock.now_iso(),
        });
        self.save_case(&mut c, vec![]).await?;
        Ok(Self::view_for(role, c))
    }

    pub async fn case_attachment(&self, user: &User, id: &str, attachment_id: &str) -> AppResult<DocumentFile> {
        let (c, _) = self.readable_case(user, id).await?;
        let a = c.attachments.iter().find(|a| a.id == attachment_id).ok_or_else(|| AppError::not_found("Attachment"))?;
        let (bytes, content_type) = self.p.files.get(&format!("cases/{}/{}", c.id, a.id)).await?;
        Ok(DocumentFile { file_name: a.file_name.clone(), content_type, bytes })
    }

    /// Worker: hand unanswered complaints to Feast; close cases the diner abandoned.
    pub async fn sweep_cases(&self) -> AppResult<usize> {
        let now = self.now();
        let mut n = 0;
        for mut c in self.p.cases.overdue_cases(&rules::iso(now)).await? {
            let Some(change) = rules::overdue_transition(&c, now) else { continue };
            match change {
                CaseChange::AutoEscalated => {
                    let text =
                        format!("{} didn't reply within {} hours, so Feast will review this complaint.", c.restaurant.name, rules::RESTAURANT_REPLY_HOURS);
                    c.messages.push(self.message(CaseRole::Feast, "Feast".into(), text, false, true));
                    c.status = CaseStatus::WithFeast;
                    c.due_at = Some(rules::iso(now + Duration::hours(rules::FEAST_DECISION_HOURS)));
                    c.escalated_at = Some(rules::iso(now));
                }
                CaseChange::AutoClosed => {
                    let at = rules::iso(now);
                    c.status = CaseStatus::Resolved;
                    c.due_at = None;
                    c.resolved_at = Some(at.clone());
                    c.outcome = Some(CaseOutcome {
                        decision: CaseDecision::SettledBetweenParties,
                        note: format!("Closed automatically: no reply from the diner within {} days.", rules::CUSTOMER_REPLY_DAYS),
                        refund_amount: None,
                        decided_by: CaseRole::Feast,
                        at,
                    });
                }
                _ => continue,
            }
            let ev = DomainEvent::CaseUpdated { case: c.reference(), change, by: CaseRole::Feast };
            match self.save_case(&mut c, vec![ev]).await {
                Ok(()) => n += 1,
                Err(e) if e.code == "CASE_CHANGED" => {} // someone acted on it meanwhile; next sweep re-checks
                Err(e) => return Err(e),
            }
        }
        Ok(n)
    }

    // ============================================================ reviews

    /// A diner reviews a visit: a completed booking or a bill paid on Feast.
    pub async fn create_review(&self, user: &User, req: CreateReviewRequest) -> AppResult<Review> {
        self.require_role(user, &[Role::Customer])?;
        if !(1..=5).contains(&req.rating) {
            return Err(AppError::validation("INVALID_RATING", "Choose 1 to 5 stars"));
        }
        let text = clean(&req.text, rules::MAX_REVIEW_CHARS + 1);
        if text.chars().count() > rules::MAX_REVIEW_CHARS {
            return Err(AppError::validation("TEXT_TOO_LONG", format!("Reviews can be at most {} characters", rules::MAX_REVIEW_CHARS)));
        }
        if req.rating <= 2 && text.chars().count() < rules::MIN_MESSAGE_CHARS {
            return Err(AppError::validation("TEXT_TOO_SHORT", "Tell the restaurant what went wrong (at least 10 characters)"));
        }
        let (restaurant, visit_date, visit_key, booking_id, payment_id) = match (&req.booking_id, &req.bill_payment_id) {
            (Some(bid), None) => {
                let b = self.p.bookings.get(bid).await?.filter(|b| b.user_id == user.id).ok_or_else(|| AppError::not_found("Booking"))?;
                let tz = self.tz_by_restaurant().await?.remove(&b.restaurant.id).unwrap_or_default();
                let b = self.effective_booking(b, &tz);
                let today = self.p.clock.local_date(&tz, 0);
                if !(b.status == BookingStatus::Completed || (b.status == BookingStatus::Seated && b.date <= today)) {
                    return Err(AppError::validation("NOT_VISITED", "You can review once you've dined there"));
                }
                (b.restaurant, b.date, format!("booking:{}", b.id), Some(b.id), None)
            }
            (None, Some(pid)) => {
                let p = self.p.billing.get_bill_payment(pid).await?.filter(|p| p.user_id == user.id).ok_or_else(|| AppError::not_found("Payment"))?;
                let tz = self.tz_by_restaurant().await?.remove(&p.restaurant.id).unwrap_or_default();
                let date = self.p.clock.local_date_of(&tz, &p.payment.paid_at).unwrap_or_default();
                (p.restaurant, date, format!("bill:{}", p.id), None, Some(p.id))
            }
            _ => return Err(AppError::validation("VISIT_REQUIRED", "Choose the visit you're reviewing")),
        };
        let tz = self.tz_by_restaurant().await?.remove(&restaurant.id).unwrap_or_default();
        if !rules::within_days(&visit_date, &self.p.clock.local_date(&tz, 0), rules::REVIEW_WITHIN_DAYS) {
            return Err(AppError::validation("REVIEW_WINDOW_CLOSED", format!("Reviews can be written within {} days of the visit", rules::REVIEW_WITHIN_DAYS)));
        }
        // A booking and the bill paid at the same visit are one visit.
        if self.p.reviews.reviews_by_user(&user.id).await?.iter().any(|r| r.restaurant.id == restaurant.id && r.visit_date == visit_date) {
            return Err(AppError::conflict("ALREADY_REVIEWED", "You've already reviewed this visit"));
        }
        let review = Review {
            id: self.p.ids.new_id("rev"),
            restaurant: restaurant.clone(),
            user_id: Some(user.id.clone()),
            author_name: rules::short_name(&user.name),
            booking_id,
            bill_payment_id: payment_id,
            visit_date,
            rating: req.rating,
            text,
            created_at: self.p.clock.now_iso(),
            reply: None,
            status: ReviewStatus::Published,
            moderation: None,
            flag: None,
        };
        let ev = self.event(
            Some(&restaurant.id),
            None,
            DomainEvent::ReviewPosted {
                review_id: review.id.clone(),
                restaurant: restaurant.clone(),
                rating: review.rating,
                author_name: review.author_name.clone(),
            },
        );
        if !self.p.reviews.insert_review(&review, &visit_key, &[ev]).await? {
            return Err(AppError::conflict("ALREADY_REVIEWED", "You've already reviewed this visit"));
        }
        self.refresh_rating(&restaurant.id).await?;
        Ok(review)
    }

    /// Recompute the restaurant's shown rating from its base + published reviews.
    async fn refresh_rating(&self, restaurant_id: &str) -> AppResult<()> {
        let Some(mut r) = self.p.restaurants.get(restaurant_id).await? else { return Ok(()) };
        let base = *r.rating_base.get_or_insert(RatingBase { average: r.detail.rating, count: r.detail.rating_count });
        let s = self.p.reviews.rating_stats(restaurant_id).await?;
        let (avg, count) = rules::blended_rating(base, s.sum, s.count);
        r.detail.rating = avg;
        r.detail.rating_count = count;
        self.p.restaurants.update(&r).await
    }

    /// Public reviews of a listed restaurant.
    pub async fn restaurant_reviews(&self, slug_or_id: &str) -> AppResult<ReviewList> {
        let r = self.public_restaurant(slug_or_id).await?;
        let s = self.p.reviews.rating_stats(&r.detail.id).await?;
        let reviews = self.p.reviews.reviews_for_restaurant(&r.detail.id, true, 50).await?.into_iter().map(Review::public).collect();
        Ok(ReviewList { summary: ReviewSummary { average: r.detail.rating, count: r.detail.rating_count, distribution: s.distribution.to_vec() }, reviews })
    }

    pub async fn my_reviews(&self, user: &User) -> AppResult<Vec<Review>> {
        Ok(self
            .p
            .reviews
            .reviews_by_user(&user.id)
            .await?
            .into_iter()
            .map(|mut r| {
                r.flag = None; // the restaurant's report is between it and Feast
                r
            })
            .collect())
    }

    /// Every review of the restaurant (including hidden ones, with Feast's reason).
    pub async fn partner_reviews(&self, user: &User, restaurant_id: &str) -> AppResult<ReviewList> {
        let r = self.managed_restaurant(user, restaurant_id).await?;
        let s = self.p.reviews.rating_stats(restaurant_id).await?;
        let reviews = self.p.reviews.reviews_for_restaurant(restaurant_id, false, 200).await?.into_iter().map(|mut x| {
            x.user_id = None;
            x
        });
        Ok(ReviewList {
            summary: ReviewSummary { average: r.detail.rating, count: r.detail.rating_count, distribution: s.distribution.to_vec() },
            reviews: reviews.collect(),
        })
    }

    async fn review(&self, id: &str) -> AppResult<Review> {
        self.p.reviews.get_review(id).await?.ok_or_else(|| AppError::not_found("Review"))
    }

    /// The restaurant answers a review publicly (replaces an earlier reply).
    pub async fn reply_review(&self, user: &User, id: &str, req: ReviewReplyRequest) -> AppResult<Review> {
        let mut rv = self.review(id).await?;
        self.editable_restaurant(user, &rv.restaurant.id).await?;
        let text = require_text(&req.text, 2, rules::MAX_REPLY_CHARS, "Your reply")?;
        rv.reply = Some(ReviewReply { text, at: self.p.clock.now_iso() });
        let ev = self.event(
            Some(&rv.restaurant.id),
            rv.user_id.as_deref(),
            DomainEvent::ReviewReplied { review_id: rv.id.clone(), restaurant: rv.restaurant.clone(), user_id: rv.user_id.clone().unwrap_or_default() },
        );
        self.p.reviews.update_review(&rv, &[ev]).await?;
        self.audit_staff_edit(user, &rv.restaurant.id, "review.replied", format!("Replied to a {}★ review on the owner's behalf", rv.rating)).await?;
        rv.user_id = None;
        Ok(rv)
    }

    /// The owner asks Feast to check a review (abusive, fake, wrong restaurant…).
    pub async fn report_review(&self, user: &User, id: &str, req: ReportReviewRequest) -> AppResult<Review> {
        let mut rv = self.review(id).await?;
        if !user.owns(&rv.restaurant.id) {
            return Err(AppError::not_found("Review"));
        }
        let reason = require_text(&req.reason, rules::MIN_REASON_CHARS, 500, "The reason")?;
        rv.flag = Some(ReviewFlag { reason: reason.clone(), at: self.p.clock.now_iso() });
        let ev = self.event(Some(&rv.restaurant.id), None, DomainEvent::ReviewReported { review_id: rv.id.clone(), restaurant: rv.restaurant.clone(), reason });
        self.p.reviews.update_review(&rv, &[ev]).await?;
        rv.user_id = None;
        Ok(rv)
    }

    pub async fn admin_reviews(&self, user: &User) -> AppResult<Vec<Review>> {
        self.require_role(user, &[Role::Admin])?;
        self.p.reviews.reviews_needing_attention().await
    }

    /// Feast hides a review that breaks the guidelines, or restores it /
    /// dismisses the restaurant's report.
    pub async fn moderate_review(&self, user: &User, id: &str, req: ModerateReviewRequest) -> AppResult<Review> {
        self.require_role(user, &[Role::Admin])?;
        let mut rv = self.review(id).await?;
        let was_hidden = rv.status == ReviewStatus::Hidden;
        let reason = match req.action {
            ModerationAction::Hide => {
                let why = require_text(req.reason.as_deref().unwrap_or_default(), rules::MIN_REASON_CHARS, 300, "The reason")?;
                rv.status = ReviewStatus::Hidden;
                rv.moderation = Some(Moderation {
                    reason: why.clone(),
                    hidden_at: self.p.clock.now_iso(),
                    hidden_by: user.id.clone(),
                    review_requested_at: None,
                    review_note: None,
                });
                Some(why)
            }
            ModerationAction::Restore => {
                rv.status = ReviewStatus::Published;
                rv.moderation = None;
                None
            }
        };
        rv.flag = None; // the report is handled either way
        let hidden = rv.status == ReviewStatus::Hidden;
        let events = if hidden != was_hidden {
            vec![self.event(
                Some(&rv.restaurant.id),
                rv.user_id.as_deref(),
                DomainEvent::ReviewModerated {
                    review_id: rv.id.clone(),
                    restaurant: rv.restaurant.clone(),
                    user_id: rv.user_id.clone().unwrap_or_default(),
                    hidden,
                    reason: reason.clone(),
                },
            )]
        } else {
            vec![]
        };
        self.p.reviews.update_review(&rv, &events).await?;
        if hidden != was_hidden {
            self.refresh_rating(&rv.restaurant.id).await?;
        }
        self.record_audit(
            user,
            &rv.restaurant.id,
            if hidden { "review.hidden" } else { "review.restored" },
            match &reason {
                Some(why) => format!("Hid a {}★ review by {}: {why}", rv.rating, rv.author_name),
                None => format!("Kept a {}★ review by {} visible", rv.rating, rv.author_name),
            },
        )
        .await?;
        Ok(rv)
    }
}
