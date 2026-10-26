//! End to end over real gRPC: client adapter → server → service → SQLite,
//! plus the `notification.created` announcement on the bus.

use std::sync::Arc;
use std::time::Duration;

use feast_application::ports::{MessageBus, NotificationDraft, NotificationGateway, SubscriptionMode};
use feast_domain::model::NotificationKind;
use feast_domain::DomainEvent;
use feast_infra::bus::memory::InMemoryBus;
use feast_infra::clock::SystemClock;
use feast_infra::ids::UuidIds;
use feast_notification::{serve, GrpcNotificationGateway, NotificationService, SqlNotificationStore};

#[tokio::test]
async fn publish_list_mark_read_over_grpc() {
    let store = Arc::new(SqlNotificationStore::connect("sqlite::memory:", 1).await.unwrap());
    let bus = Arc::new(InMemoryBus::new(64));
    let mut live = bus.subscribe(SubscriptionMode::Broadcast).await.unwrap();
    let svc = Arc::new(NotificationService::new(store, bus.clone(), Arc::new(UuidIds), Arc::new(SystemClock::from_name("Asia/Kolkata").unwrap())));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(serve(listener, svc, Some("s3cret".into()), async {
        let _ = stopped.await;
    }));

    let client = GrpcNotificationGateway::new(&format!("http://{addr}"), "test", Some("s3cret"), Duration::from_secs(5)).unwrap();
    let draft = NotificationDraft {
        kind: NotificationKind::BookingCreated,
        title: "New booking".into(),
        body: "Kavya · 4 guests · Sat 8:00 PM".into(),
        link: Some("/partner/bookings".into()),
        restaurant_id: Some("r_olive_ember".into()),
    };
    let owners = vec!["u_owner".to_string(), "u_owner".to_string(), "u_admin".to_string()];
    let created = client.publish("evt_1", "booking.created", &owners, &draft).await.unwrap();
    assert_eq!(created.len(), 2, "recipients de-duplicated");
    // Redelivery of the same event creates nothing new (idempotent).
    assert!(client.publish("evt_1", "booking.created", &owners, &draft).await.unwrap().is_empty());

    // Each new notification was announced for realtime delivery.
    for _ in 0..2 {
        let d = tokio::time::timeout(Duration::from_secs(2), live.next()).await.unwrap().unwrap().unwrap();
        assert!(matches!(d.envelope.event, DomainEvent::NotificationCreated { .. }));
    }

    let list = client.list("u_owner", false, 20).await.unwrap();
    assert_eq!((list.items.len(), list.unread), (1, 1));
    assert_eq!(list.items[0].link.as_deref(), Some("/partner/bookings"));
    assert_eq!(client.mark_read("u_owner", &[]).await.unwrap(), 1);
    assert_eq!(client.unread_count("u_owner").await.unwrap(), 0);
    assert_eq!(client.unread_count("u_admin").await.unwrap(), 1);

    // Wrong service token is rejected.
    let intruder = GrpcNotificationGateway::new(&format!("http://{addr}"), "x", Some("wrong"), Duration::from_secs(5)).unwrap();
    assert_eq!(intruder.unread_count("u_admin").await.unwrap_err().kind, feast_application::ErrorKind::Unauthenticated);

    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}
