//! Redis Streams bus against a real Redis. Skipped unless FEAST_TEST_REDIS_URL is set:
//!   FEAST_TEST_REDIS_URL=redis://127.0.0.1:6379 cargo test -p feast-infra --test redis_bus

use std::time::Duration;

use feast_application::ports::{MessageBus, SubscriptionMode};
use feast_domain::events::DomainEvent;
use feast_domain::EventEnvelope;
use feast_infra::bus::redis::RedisStreamsBus;
use feast_infra::bus::RedisBusConfig;

fn ev(n: u32) -> EventEnvelope {
    EventEnvelope::new(
        format!("evt_{n}"),
        "2026-01-01T00:00:00.000Z".into(),
        Some("r_1".into()),
        None,
        DomainEvent::DealReleased { bill_id: format!("bill_{n}"), intent_id: format!("pi_{n}"), reason: "test".into() },
    )
}

async fn bus(stream: &str) -> Option<RedisStreamsBus> {
    let url = std::env::var("FEAST_TEST_REDIS_URL").ok()?;
    let cfg = RedisBusConfig { url, stream: stream.into(), max_len: 10_000, block_ms: 200, claim_idle_ms: 300 };
    Some(RedisStreamsBus::connect(cfg).await.unwrap())
}

#[tokio::test]
async fn group_delivery_ack_and_crash_recovery() {
    let stream = format!("feast:test:{}", uuid::Uuid::now_v7().simple());
    let Some(bus) = bus(&stream).await else { return };
    let group = |c: &str| SubscriptionMode::Group { group: "projector".into(), consumer: c.into() };
    let mut a = bus.subscribe(group("a")).await.unwrap();
    let mut fan = bus.subscribe(SubscriptionMode::Broadcast).await.unwrap();
    for n in 0..3 {
        bus.publish(&ev(n)).await.unwrap();
    }
    // Consumer a receives 0 and 1 but acks only 0, then "crashes".
    let d0 = a.next().await.unwrap().unwrap();
    a.ack(&d0).await.unwrap();
    let d1 = a.next().await.unwrap().unwrap();
    assert_eq!((d0.envelope.id.as_str(), d1.envelope.id.as_str()), ("evt_0", "evt_1"));
    drop(a);

    // Restarted consumer a: re-delivers its unacked evt_1 first, then evt_2.
    let mut a2 = bus.subscribe(group("a")).await.unwrap();
    let r1 = a2.next().await.unwrap().unwrap();
    assert_eq!(r1.envelope.id, "evt_1");
    a2.ack(&r1).await.unwrap();
    let r2 = a2.next().await.unwrap().unwrap();
    assert_eq!(r2.envelope, ev(2));
    drop(a2); // evt_2 left unacked on a dead consumer

    // A different consumer claims it once it's been idle long enough.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let mut b = bus.subscribe(group("b")).await.unwrap();
    let c = tokio::time::timeout(Duration::from_secs(3), b.next()).await.unwrap().unwrap().unwrap();
    assert_eq!(c.envelope.id, "evt_2");
    b.ack(&c).await.unwrap();

    // Broadcast saw everything published after it subscribed.
    for n in 0..3 {
        assert_eq!(fan.next().await.unwrap().unwrap().envelope.id, format!("evt_{n}"));
    }
}
