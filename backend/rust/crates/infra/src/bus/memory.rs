//! In-process message bus. Same semantics as the Redis adapter, minus durability:
//! events published before a group subscribes (or while the process is down)
//! are not delivered to it. Use only when everything runs in one process.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use feast_application::ports::{Delivery, MessageBus, Subscription, SubscriptionMode};
use feast_application::{AppError, AppResult};
use feast_domain::EventEnvelope;
use tokio::sync::{broadcast, Notify};

#[derive(Default)]
struct Group {
    queue: Mutex<VecDeque<Delivery>>,
    notify: Notify,
}

pub struct InMemoryBus {
    fanout: broadcast::Sender<Delivery>,
    groups: Mutex<HashMap<String, Arc<Group>>>,
    seq: std::sync::atomic::AtomicU64,
}

impl InMemoryBus {
    pub fn new(capacity: usize) -> Self {
        let (fanout, _) = broadcast::channel(capacity.max(16));
        Self { fanout, groups: Mutex::new(HashMap::new()), seq: Default::default() }
    }
}

#[async_trait]
impl MessageBus for InMemoryBus {
    async fn publish(&self, event: &EventEnvelope) -> AppResult<()> {
        let n = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let d = Delivery { id: n.to_string(), envelope: event.clone() };
        let groups: Vec<Arc<Group>> = self.groups.lock().map_err(AppError::internal)?.values().cloned().collect();
        for g in groups {
            g.queue.lock().map_err(AppError::internal)?.push_back(d.clone());
            g.notify.notify_one();
        }
        let _ = self.fanout.send(d); // no broadcast subscribers is fine
        Ok(())
    }

    async fn subscribe(&self, mode: SubscriptionMode) -> AppResult<Box<dyn Subscription>> {
        Ok(match mode {
            SubscriptionMode::Group { group, .. } => {
                let g = self.groups.lock().map_err(AppError::internal)?.entry(group).or_default().clone();
                Box::new(GroupSub { group: g })
            }
            SubscriptionMode::Broadcast => Box::new(BroadcastSub { rx: self.fanout.subscribe() }),
        })
    }
}

struct GroupSub {
    group: Arc<Group>,
}

#[async_trait]
impl Subscription for GroupSub {
    async fn next(&mut self) -> AppResult<Option<Delivery>> {
        loop {
            // Register interest before checking, so a publish in between isn't missed.
            let notified = self.group.notify.notified();
            if let Some(d) = self.group.queue.lock().map_err(AppError::internal)?.pop_front() {
                return Ok(Some(d));
            }
            notified.await;
        }
    }
    async fn ack(&mut self, _d: &Delivery) -> AppResult<()> {
        Ok(())
    }
}

struct BroadcastSub {
    rx: broadcast::Receiver<Delivery>,
}

#[async_trait]
impl Subscription for BroadcastSub {
    async fn next(&mut self) -> AppResult<Option<Delivery>> {
        loop {
            match self.rx.recv().await {
                Ok(d) => return Ok(Some(d)),
                Err(broadcast::error::RecvError::Lagged(n)) => tracing::warn!(skipped = n, "broadcast subscriber lagged; live updates dropped"),
                Err(broadcast::error::RecvError::Closed) => return Ok(None),
            }
        }
    }
    async fn ack(&mut self, _d: &Delivery) -> AppResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use feast_domain::events::DomainEvent;

    fn ev(n: u32) -> EventEnvelope {
        EventEnvelope::new(
            format!("evt_{n}"),
            "2026-01-01T00:00:00.000Z".into(),
            Some("r_1".into()),
            None,
            DomainEvent::DealReleased { bill_id: format!("bill_{n}"), intent_id: format!("pi_{n}"), reason: "test".into() },
        )
    }

    #[tokio::test]
    async fn groups_compete_and_broadcast_fans_out() {
        let bus = InMemoryBus::new(64);
        let mut a = bus.subscribe(SubscriptionMode::Group { group: "g".into(), consumer: "a".into() }).await.unwrap();
        let mut b = bus.subscribe(SubscriptionMode::Group { group: "g".into(), consumer: "b".into() }).await.unwrap();
        let mut other = bus.subscribe(SubscriptionMode::Group { group: "h".into(), consumer: "x".into() }).await.unwrap();
        let mut f1 = bus.subscribe(SubscriptionMode::Broadcast).await.unwrap();
        let mut f2 = bus.subscribe(SubscriptionMode::Broadcast).await.unwrap();
        for n in 0..4 {
            bus.publish(&ev(n)).await.unwrap();
        }
        // Group g: 4 events split across a and b, each delivered once.
        let mut seen = vec![];
        for _ in 0..2 {
            seen.push(a.next().await.unwrap().unwrap().envelope.id);
            seen.push(b.next().await.unwrap().unwrap().envelope.id);
        }
        seen.sort();
        assert_eq!(seen, vec!["evt_0", "evt_1", "evt_2", "evt_3"]);
        // Group h gets its own copy of everything.
        assert_eq!(other.next().await.unwrap().unwrap().envelope.id, "evt_0");
        // Every broadcast subscriber sees every event.
        assert_eq!(f1.next().await.unwrap().unwrap().envelope.id, "evt_0");
        assert_eq!(f2.next().await.unwrap().unwrap().envelope.id, "evt_0");
    }

    #[tokio::test]
    async fn group_waits_for_publish() {
        let bus = Arc::new(InMemoryBus::new(16));
        let mut s = bus.subscribe(SubscriptionMode::Group { group: "g".into(), consumer: "a".into() }).await.unwrap();
        let b2 = bus.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            b2.publish(&ev(9)).await.unwrap();
        });
        let d = tokio::time::timeout(std::time::Duration::from_secs(2), s.next()).await.unwrap().unwrap().unwrap();
        assert_eq!(d.envelope.id, "evt_9");
    }
}
