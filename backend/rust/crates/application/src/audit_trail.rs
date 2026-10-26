//! Reading the audit trail: admins see everything, owners their restaurant's,
//! diners the history of their own bookings and payments.

use feast_domain::audit::{page_size, AuditPage, AuditQuery};
use feast_domain::model::*;

use crate::context::Feast;
use crate::error::{AppError, AppResult};

/// Filters a client may set (the use case decides the scope).
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFilter {
    pub restaurant_id: Option<String>,
    pub entity_type: Option<String>,
    pub entity_id: Option<String>,
    pub actor_id: Option<String>,
    pub user_id: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty() && s.len() <= 120)
}

impl AuditFilter {
    fn into_query(self) -> AuditQuery {
        AuditQuery {
            restaurant_id: clean(self.restaurant_id),
            entity_type: clean(self.entity_type),
            entity_id: clean(self.entity_id),
            entity_ids: vec![],
            actor_id: clean(self.actor_id),
            subject_user_id: clean(self.user_id),
            from: clean(self.from),
            to: clean(self.to),
            cursor: clean(self.cursor),
            limit: page_size(self.limit),
        }
    }
}

impl Feast {
    /// Admin audit explorer: every record, any filter.
    pub async fn admin_audit(&self, user: &User, f: AuditFilter) -> AppResult<AuditPage> {
        self.require_role(user, &[Role::Admin])?;
        self.p.trail.query(&f.into_query()).await
    }

    /// A restaurant's activity log (its owner, or Feast staff).
    pub async fn restaurant_audit(&self, user: &User, restaurant_id: &str, f: AuditFilter) -> AppResult<AuditPage> {
        self.managed_restaurant(user, restaurant_id).await?;
        let mut q = f.into_query();
        q.restaurant_id = Some(restaurant_id.to_owned());
        self.p.trail.query(&q).await
    }

    /// History of one of the diner's own bookings.
    pub async fn booking_history(&self, user: &User, booking_id: &str) -> AppResult<AuditPage> {
        let b = self.p.bookings.get(booking_id).await?.ok_or_else(|| AppError::not_found("Booking"))?;
        if b.user_id != user.id && !user.can_manage(&b.restaurant.id) {
            return Err(AppError::not_found("Booking"));
        }
        self.p
            .trail
            .query(&AuditQuery { entity_type: Some("booking".into()), entity_id: Some(b.id), limit: page_size(Some(100)), ..AuditQuery::default() })
            .await
    }

    /// History of one bill payment: the bill (deal held/used), the payment
    /// attempts, any counter payment and the receipt itself.
    pub async fn bill_payment_history(&self, user: &User, bill_payment_id: &str) -> AppResult<AuditPage> {
        let bp = self.bill_payment(user, bill_payment_id).await?;
        let mut ids = vec![bp.id.clone(), bp.bill.id.clone(), bp.payment.intent_id.clone()];
        if let Some(op) = self.p.offline.latest_for_bill(&bp.bill.id).await? {
            ids.push(op.id);
            ids.push(op.intent_id);
        }
        ids.sort();
        ids.dedup();
        self.p.trail.query(&AuditQuery { entity_ids: ids, limit: page_size(Some(100)), ..AuditQuery::default() }).await
    }
}
