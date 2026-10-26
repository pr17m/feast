//! Live updates: bus → this gateway → each connected client (SSE).
//!
//! Every gateway instance holds one *broadcast* subscription on the bus and
//! fans events out in-process to its SSE connections, filtered per user. With
//! Redis Streams that means every instance sees every event, so a customer on
//! instance A booking a table reaches the owner connected to instance B.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use feast_application::ports::{MessageBus, SubscriptionMode};
use feast_domain::model::{Role, User};
use feast_domain::{DomainEvent, EventEnvelope};
use serde_json::json;
use tokio::sync::{broadcast, watch};

pub struct RealtimeHub {
    tx: broadcast::Sender<Arc<EventEnvelope>>,
    connections: AtomicUsize,
    closed: watch::Sender<bool>,
}

/// What a client receives.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveEvent {
    /// A new notification for this user (`event: notification`).
    Notification(serde_json::Value),
    /// Something this user can see changed — refetch (`event: change`).
    Change(serde_json::Value),
}

impl RealtimeHub {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(64));
        Self { tx, connections: AtomicUsize::new(0), closed: watch::channel(false).0 }
    }

    /// End every open stream (call on shutdown so graceful shutdown can finish).
    pub fn close(&self) {
        let _ = self.closed.send(true);
    }

    /// Resolves once [`close`](Self::close) has been called.
    pub(crate) fn closed(&self) -> impl std::future::Future<Output = ()> + Send + 'static {
        let mut rx = self.closed.subscribe();
        async move {
            let _ = rx.wait_for(|c| *c).await;
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Arc<EventEnvelope>> {
        self.tx.subscribe()
    }

    pub fn publish(&self, env: EventEnvelope) {
        let _ = self.tx.send(Arc::new(env));
    }

    pub fn connections(&self) -> usize {
        self.connections.load(Ordering::Relaxed)
    }

    /// Count a live connection until the guard drops.
    pub(crate) fn connected(self: &Arc<Self>) -> ConnectionGuard {
        self.connections.fetch_add(1, Ordering::Relaxed);
        ConnectionGuard(self.clone())
    }

    /// Pump events from the bus into the hub until shutdown; resubscribes on errors.
    pub async fn run(self: Arc<Self>, bus: Arc<dyn MessageBus>, mut shutdown: watch::Receiver<bool>) {
        loop {
            let mut sub = match bus.subscribe(SubscriptionMode::Broadcast).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "realtime: bus subscribe failed; retrying");
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(2)) => continue,
                        _ = shutdown.changed() => return,
                    }
                }
            };
            tracing::info!("realtime hub listening for live events");
            loop {
                tokio::select! {
                    next = sub.next() => match next {
                        Ok(Some(d)) => self.publish(d.envelope),
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!(error = %e, "realtime: bus read failed; resubscribing");
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            break;
                        }
                    },
                    _ = shutdown.changed() => return,
                }
            }
        }
    }
}

pub(crate) struct ConnectionGuard(Arc<RealtimeHub>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Decide what (if anything) `user` should receive for `env`.
pub fn route_for(user: &User, env: &EventEnvelope) -> Option<LiveEvent> {
    if let DomainEvent::NotificationCreated { notification } = &env.event {
        return (notification.user_id == user.id).then(|| LiveEvent::Notification(serde_json::to_value(notification).unwrap_or_default()));
    }
    let mine = env.user_id.as_deref() == Some(user.id.as_str());
    let manages = env.restaurant_id.as_deref().is_some_and(|r| match user.role {
        Role::Admin => true,
        Role::Owner => user.restaurant_ids.iter().any(|x| x == r),
        Role::Customer => false,
    });
    let touches_me = match &env.event {
        DomainEvent::BookingStatusChanged { booking, .. } | DomainEvent::BookingCancelled { booking, .. } => booking.user_id == user.id,
        _ => false,
    };
    (mine || manages || touches_me).then(|| LiveEvent::Change(json!({ "type": env.type_name(), "restaurantId": env.restaurant_id, "eventId": env.id })))
}
