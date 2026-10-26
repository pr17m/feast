//! Notification business logic. Implements the core's `NotificationGateway`
//! port directly, so "embedded" mode is just this struct.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use feast_application::ports::{Clock, IdGenerator, MessageBus, NotificationDraft, NotificationGateway};
use feast_application::AppResult;
use feast_domain::model::{Notification, NotificationList};
use feast_domain::{DomainEvent, EventEnvelope};

use crate::store::NotificationStore;

pub struct NotificationService {
    store: Arc<dyn NotificationStore>,
    /// Where `notification.created` is announced for realtime delivery.
    bus: Arc<dyn MessageBus>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl NotificationService {
    pub fn new(store: Arc<dyn NotificationStore>, bus: Arc<dyn MessageBus>, ids: Arc<dyn IdGenerator>, clock: Arc<dyn Clock>) -> Self {
        Self { store, bus, ids, clock }
    }

    pub async fn ping(&self) -> AppResult<()> {
        self.store.ping().await
    }
}

#[async_trait]
impl NotificationGateway for NotificationService {
    async fn publish(&self, event_id: &str, event_type: &str, recipients: &[String], d: &NotificationDraft) -> AppResult<Vec<Notification>> {
        let now = self.clock.now_iso();
        let mut created = vec![];
        for user_id in recipients.iter().filter(|r| !r.is_empty()).collect::<BTreeSet<_>>() {
            let n = Notification {
                id: self.ids.new_id("ntf"),
                user_id: user_id.clone(),
                restaurant_id: d.restaurant_id.clone(),
                kind: d.kind,
                title: d.title.clone(),
                body: d.body.clone(),
                link: d.link.clone(),
                event_id: event_id.to_owned(),
                created_at: now.clone(),
                read_at: None,
            };
            if self.store.insert_if_absent(&n, event_type).await? {
                created.push(n);
            }
        }
        // Realtime fan-out. Best effort: the notification is already stored, and
        // clients re-fetch the list when they (re)connect.
        for n in &created {
            let env = EventEnvelope::new(
                self.ids.new_id("evt"),
                now.clone(),
                n.restaurant_id.clone(),
                Some(n.user_id.clone()),
                DomainEvent::NotificationCreated { notification: n.clone() },
            );
            if let Err(e) = self.bus.publish(&env).await {
                tracing::warn!(notification = %n.id, error = %e, "could not announce notification for live delivery");
            }
        }
        Ok(created)
    }

    async fn list(&self, user_id: &str, unread_only: bool, limit: u32) -> AppResult<NotificationList> {
        Ok(NotificationList { items: self.store.list(user_id, unread_only, limit).await?, unread: self.store.unread_count(user_id).await? })
    }

    async fn mark_read(&self, user_id: &str, ids: &[String]) -> AppResult<i64> {
        self.store.mark_read(user_id, ids, &self.clock.now_iso()).await
    }

    async fn unread_count(&self, user_id: &str) -> AppResult<i64> {
        self.store.unread_count(user_id).await
    }
}
