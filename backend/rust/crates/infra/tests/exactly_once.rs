//! Money and side effects happen once, even with several API instances and
//! workers (separate connection pools) on the same database:
//!
//! - a payment that arrives after its booking was cancelled (or its table
//!   sold) never confirms it; it is recorded once for a refund;
//! - of several simultaneous cancellations of a booking, one wins;
//! - several outbox relays publish each event exactly once (database lease);
//! - rate-limit counters, one-time codes and leases are shared by instances.
//!
//! Runs on SQLite always, and on Postgres when FEAST_TEST_POSTGRES_URL is set
//! (the relay test creates and drops its own database there):
//!   FEAST_TEST_POSTGRES_URL=postgres://feast:feast@127.0.0.1:5433/feast_test cargo test -p feast-infra --test exactly_once

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use feast_application::ports::{Clock, MessageBus, Settled, SlotRoom, Subscription, SubscriptionMode};
use feast_application::processes::{run_outbox_relay, Lease, RelayConfig};
use feast_application::{AppError, AppResult};
use feast_domain::audit::AuditQuery;
use feast_domain::events::Party;
use feast_domain::model::*;
use feast_domain::refunds::{Refund, RefundReason, RefundStatus};
use feast_domain::{DomainEvent, EventEnvelope};
use feast_infra::clock::SystemClock;
use feast_infra::database::{connect, DatabaseConfig, Repositories};

fn unique() -> String {
    format!("{}{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

fn iso(t: chrono::DateTime<chrono::Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

async fn pools(url: &str, n: usize) -> Vec<Repositories> {
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(connect(&DatabaseConfig { url: url.into(), max_connections: 8 }).await.unwrap());
    }
    v
}

fn sqlite_url(name: &str) -> (String, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("feast-{name}-{}", unique()));
    std::fs::create_dir_all(&dir).unwrap();
    (format!("sqlite://{}", dir.join("db.sqlite").display()), dir)
}

// ------------------------------------------------------------ late payments

fn booking(id: &str, rid: &str, created_at: &str) -> Booking {
    Booking {
        id: id.into(),
        user_id: "u_diner".into(),
        code: "FST-0001".into(),
        restaurant: RestaurantRef {
            id: rid.into(),
            slug: rid.into(),
            name: "Late".into(),
            area: "CBD".into(),
            art: Art { hue: 1, motif: CuisineMotif::Cafe, image: None, image_credit: None },
        },
        date: "2031-02-01".into(),
        time: "19:00".into(),
        guests: 2,
        deal: None,
        special_request: None,
        contact: Contact { name: "D".into(), phone: "0400000000".into() },
        status: BookingStatus::PendingPayment,
        cover_charge: 2000,
        currency: "AUD".into(),
        payment: None,
        created_at: created_at.into(),
        checked_in_at: None,
    }
}

fn intent(id: &str, booking_id: &str) -> PaymentIntent {
    PaymentIntent {
        id: id.into(),
        purpose: PaymentPurpose::Booking,
        reference_id: booking_id.into(),
        amount: 2000,
        currency: "AUD".into(),
        provider: "mock".into(),
        checkout: None,
        provider_ref: Some(format!("mock_{id}")),
        plan_id: None,
        user_id: Some("u_diner".into()),
        status: PaymentStatus::Created,
        result_ref: None,
        failure_reason: None,
        provider_payment_id: None,
    }
}

fn succeeded(i: &PaymentIntent) -> PaymentIntent {
    let mut i = i.clone();
    i.status = PaymentStatus::Succeeded;
    i
}

fn confirmed(b: &Booking) -> Booking {
    let mut b = b.clone();
    b.status = BookingStatus::Confirmed;
    b
}

fn refund_for(i: &PaymentIntent) -> Refund {
    Refund {
        id: format!("rf_{}", i.id),
        intent_id: i.id.clone(),
        purpose: i.purpose,
        reference_id: i.reference_id.clone(),
        user_id: i.user_id.clone(),
        restaurant_id: None,
        amount: i.amount,
        currency: i.currency.clone(),
        provider: i.provider.clone(),
        provider_refund_id: None,
        status: RefundStatus::Pending,
        reason: RefundReason::PaidAfterCancellation,
        idempotency_key: format!("unapplied:{}", i.id),
        note: None,
        case_id: None,
        requested_by: None,
        failure: None,
        attempts: 0,
        created_at: iso(chrono::Utc::now()),
        updated_at: iso(chrono::Utc::now()),
    }
}

async fn late_payments(url: &str) {
    let repos = pools(url, 2).await;
    let (a, b) = (&repos[0], &repos[1]);
    let rid = format!("r_late_{}", unique());
    let now = chrono::Utc::now();
    let fresh = iso(now);
    let room = |tables| SlotRoom { tables, pending_since: iso(now - chrono::Duration::minutes(20)) };

    // 1. Cancelled while the diner was paying: the payment can't bring it back.
    let bk = booking(&format!("{rid}_cancelled"), &rid, &fresh);
    assert!(a.bookings.insert_if_room(&bk, &format!("{rid}-k1"), &[], &room(5)).await.unwrap());
    let mut cancelled = bk.clone();
    cancelled.status = BookingStatus::Cancelled;
    a.bookings.update(&cancelled, &[]).await.unwrap();
    let pi = intent(&format!("pi_{rid}_1"), &bk.id);
    a.billing.insert_intent(&pi, &[]).await.unwrap();
    let got = b.billing.complete_booking_payment(&succeeded(&pi), &confirmed(&bk), &room(5), &[]).await.unwrap();
    assert!(matches!(got, Settled::NotApplied), "a cancelled booking is never confirmed by a late payment: {got:?}");
    assert_eq!(a.bookings.get(&bk.id).await.unwrap().unwrap().status, BookingStatus::Cancelled);
    // Recorded once for a refund, however many times the provider tells us.
    let r = refund_for(&pi);
    assert!(a.billing.record_unapplied_payment(&succeeded(&pi), &r, &[]).await.unwrap());
    assert!(!b.billing.record_unapplied_payment(&succeeded(&pi), &r, &[]).await.unwrap(), "second webhook: nothing new");
    assert_eq!(a.refunds.refunds_for_intent(&pi.id).await.unwrap().len(), 1);
    // And it is now settled: a later confirm is a no-op.
    let again = a.billing.complete_booking_payment(&succeeded(&pi), &confirmed(&bk), &room(5), &[]).await.unwrap();
    assert!(matches!(again, Settled::AlreadySettled), "{again:?}");

    // 2. Hold lapsed and the last table went to someone else: not applied.
    let old = iso(now - chrono::Duration::minutes(40));
    let slow = booking(&format!("{rid}_slow"), &rid, &old);
    let mut slow2 = slow.clone();
    slow2.time = "20:00".into();
    assert!(a.bookings.insert_if_room(&slow2, &format!("{rid}-k2"), &[], &room(1)).await.unwrap());
    let mut other = booking(&format!("{rid}_other"), &rid, &fresh);
    other.time = "20:00".into();
    assert!(a.bookings.insert_if_room(&other, &format!("{rid}-k3"), &[], &room(1)).await.unwrap(), "lapsed hold frees the table");
    let pi2 = intent(&format!("pi_{rid}_2"), &slow2.id);
    a.billing.insert_intent(&pi2, &[]).await.unwrap();
    let got = a.billing.complete_booking_payment(&succeeded(&pi2), &confirmed(&slow2), &room(1), &[]).await.unwrap();
    assert!(matches!(got, Settled::NotApplied), "table sold after the hold lapsed: {got:?}");
    assert_eq!(a.bookings.get(&slow2.id).await.unwrap().unwrap().status, BookingStatus::PendingPayment);

    // 3. Hold lapsed but a table is still free: confirmed as before.
    let mut lucky = booking(&format!("{rid}_lucky"), &rid, &old);
    lucky.time = "21:00".into();
    assert!(a.bookings.insert_if_room(&lucky, &format!("{rid}-k4"), &[], &room(2)).await.unwrap());
    let pi3 = intent(&format!("pi_{rid}_3"), &lucky.id);
    a.billing.insert_intent(&pi3, &[]).await.unwrap();
    let got = b.billing.complete_booking_payment(&succeeded(&pi3), &confirmed(&lucky), &room(2), &[]).await.unwrap();
    assert!(matches!(got, Settled::Applied), "{got:?}");
    assert_eq!(a.bookings.get(&lucky.id).await.unwrap().unwrap().status, BookingStatus::Confirmed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn late_payments_never_resurrect_bookings_sqlite() {
    let (url, dir) = sqlite_url("late");
    late_payments(&url).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn late_payments_never_resurrect_bookings_postgres() {
    let Ok(url) = std::env::var("FEAST_TEST_POSTGRES_URL") else { return };
    late_payments(&url).await;
}

// ------------------------------------------------------------ cancellations

/// The diner and the restaurant (and a retry) cancel the same booking at once,
/// on different instances: one wins, so there is one event (one notification,
/// one audit record, one refund).
async fn cancel_race(url: &str) {
    let repos = pools(url, 3).await;
    let rid = format!("r_cancel_{}", unique());
    let room = SlotRoom { tables: 5, pending_since: iso(chrono::Utc::now() - chrono::Duration::minutes(20)) };
    let bk = booking(&format!("{rid}_b"), &rid, &iso(chrono::Utc::now()));
    assert!(repos[0].bookings.insert_if_room(&bk, &format!("{rid}-k"), &[], &room).await.unwrap());
    let live = confirmed(&bk);
    repos[0].bookings.update(&live, &[]).await.unwrap();

    let mut tasks = Vec::new();
    for n in 0..9 {
        let r = repos[n % 3].clone();
        let mut c = live.clone();
        c.status = BookingStatus::Cancelled;
        let by = [Party::Customer, Party::Restaurant, Party::Admin][n % 3];
        let ev = EventEnvelope::new(
            format!("evt_{rid}_{n}"),
            iso(chrono::Utc::now()),
            Some(rid.clone()),
            Some("u_diner".into()),
            DomainEvent::BookingCancelled { booking: c.clone(), by },
        );
        tasks.push(tokio::spawn(async move { r.bookings.update_if_status(&c, BookingStatus::Confirmed, &[ev]).await.unwrap() }));
    }
    let mut won = 0;
    for t in tasks {
        won += t.await.unwrap() as usize;
    }
    assert_eq!(won, 1, "exactly one cancellation wins");
    assert_eq!(repos[1].bookings.get(&bk.id).await.unwrap().unwrap().status, BookingStatus::Cancelled);
    let trail = repos[2]
        .trail
        .query(&AuditQuery { restaurant_id: Some(rid.clone()), entity_id: Some(bk.id.clone()), limit: 50, ..AuditQuery::default() })
        .await
        .unwrap();
    assert_eq!(trail.items.iter().filter(|r| r.action == "booking.cancelled").count(), 1, "one audit record (and one event to notify from)");

    // A cancelled (or checked-in) booking can't be cancelled again.
    let mut again = live.clone();
    again.status = BookingStatus::Cancelled;
    assert!(!repos[0].bookings.update_if_status(&again, BookingStatus::Confirmed, &[]).await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_cancellation_wins_sqlite() {
    let (url, dir) = sqlite_url("cancel");
    cancel_race(&url).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_cancellation_wins_postgres() {
    let Ok(url) = std::env::var("FEAST_TEST_POSTGRES_URL") else { return };
    cancel_race(&url).await;
}

// ------------------------------------------------------------ outbox relays

/// A bus that records what was published (shared by every relay in the test).
struct Recorder(Arc<Mutex<HashMap<String, usize>>>);

#[async_trait]
impl MessageBus for Recorder {
    async fn publish(&self, event: &EventEnvelope) -> AppResult<()> {
        // A little latency so relays overlap if nothing coordinates them.
        tokio::time::sleep(Duration::from_millis(2)).await;
        *self.0.lock().unwrap().entry(event.id.clone()).or_default() += 1;
        Ok(())
    }
    async fn subscribe(&self, _mode: SubscriptionMode) -> AppResult<Box<dyn Subscription>> {
        Err(AppError::unavailable("recorder bus"))
    }
}

const EVENTS: usize = 120;
const RELAYS: usize = 3;

async fn relays(url: &str) {
    let repos = pools(url, RELAYS).await;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::from_name("Australia/Sydney").unwrap());
    let ids: Vec<String> = (0..EVENTS).map(|n| format!("evt_relay_{}_{n}", unique())).collect();
    for chunk in ids.chunks(10) {
        let evs: Vec<EventEnvelope> = chunk
            .iter()
            .map(|id| {
                let mut r = refund_for(&intent(id, "b"));
                r.idempotency_key = id.clone();
                EventEnvelope::new(id.clone(), iso(chrono::Utc::now()), None, None, DomainEvent::RefundRequested { refund: r })
            })
            .collect();
        repos[0].outbox.append(&evs).await.unwrap();
    }
    let seen = Arc::new(Mutex::new(HashMap::new()));
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let mut tasks = Vec::new();
    for (n, r) in repos.iter().enumerate() {
        let lease = Lease { store: r.shared.clone(), clock: clock.clone(), name: "outbox-relay", holder: format!("relay-{n}"), ttl: Duration::from_secs(15) };
        tasks.push(tokio::spawn(run_outbox_relay(
            r.outbox.clone(),
            Arc::new(Recorder(seen.clone())),
            RelayConfig { poll_interval: Duration::from_millis(10), batch_size: 7 },
            lease,
            stop_rx.clone(),
        )));
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        let done = { ids.iter().all(|id| seen.lock().unwrap().contains_key(id)) };
        if done || std::time::Instant::now() > deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    // Let any straggling relay run a few more ticks before stopping.
    tokio::time::sleep(Duration::from_millis(300)).await;
    stop_tx.send(true).unwrap();
    for t in tasks {
        t.await.unwrap();
    }
    let seen = seen.lock().unwrap().clone();
    for id in &ids {
        assert_eq!(seen.get(id).copied(), Some(1), "event {id} must be published exactly once");
    }
    assert!(repos[1].outbox.fetch_pending(10).await.unwrap().iter().all(|(_, e)| !ids.contains(&e.id)), "nothing left pending");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn several_relays_publish_each_event_once_sqlite() {
    let (url, dir) = sqlite_url("relay");
    relays(&url).await;
    let _ = std::fs::remove_dir_all(dir);
}

/// Its own database: the relays publish every pending row, not just ours.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn several_relays_publish_each_event_once_postgres() {
    let Ok(base) = std::env::var("FEAST_TEST_POSTGRES_URL") else { return };
    sqlx::any::install_default_drivers();
    let name = format!("feast_relay_{}", unique());
    let admin = sqlx::AnyPool::connect(&base).await.unwrap();
    sqlx::query(&format!("CREATE DATABASE {name}")).execute(&admin).await.unwrap();
    let url = format!("{}/{name}", base.rsplit_once('/').unwrap().0);
    relays(&url).await;
    sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)")).execute(&admin).await.unwrap();
}

// ------------------------------------------------------------ push, shared state

async fn shared_state(url: &str) {
    let repos = pools(url, 3).await;
    let tag = unique();

    // (Push delivery once per phone across workers: tests/push_retry.rs.)

    // Rate limit counter shared by instances (same window).
    let key = format!("login|{tag}");
    let mut hits = Vec::new();
    for n in 0..9 {
        hits.push(repos[n % 3].shared.rate_hit(&key, 42, i64::MAX).await.unwrap());
    }
    assert_eq!(hits, (1..=9).collect::<Vec<i64>>());
    assert_eq!(repos[0].shared.rate_hit(&key, 43, i64::MAX).await.unwrap(), 1, "new window starts over");

    // One-time code: issued on one instance, verified on another, used once.
    let phone = format!("+61{tag}");
    repos[0].shared.otp_put(&phone, "h1", 10_000).await.unwrap();
    assert!(!repos[1].shared.otp_check(&phone, "bad", 5_000, 5).await.unwrap());
    assert!(repos[2].shared.otp_check(&phone, "h1", 5_000, 5).await.unwrap());
    assert!(!repos[1].shared.otp_check(&phone, "h1", 5_000, 5).await.unwrap(), "codes work once");
    repos[0].shared.otp_put(&phone, "h2", 10_000).await.unwrap();
    assert!(!repos[1].shared.otp_check(&phone, "h2", 10_001, 5).await.unwrap(), "expired");

    // Lease: one holder until it expires.
    let lease = format!("lease_{tag}");
    assert!(repos[0].shared.lease(&lease, "a", 0, 1_000).await.unwrap());
    assert!(!repos[1].shared.lease(&lease, "b", 500, 1_500).await.unwrap());
    assert!(repos[0].shared.lease(&lease, "a", 900, 1_900).await.unwrap(), "holder renews");
    assert!(repos[1].shared.lease(&lease, "b", 2_000, 3_000).await.unwrap(), "taken over after expiry");
    assert!(!repos[0].shared.lease(&lease, "a", 2_100, 3_100).await.unwrap());

    assert!(repos[0].shared.purge_expired(20_000).await.unwrap() >= 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_state_across_instances_sqlite() {
    let (url, dir) = sqlite_url("shared");
    shared_state(&url).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_state_across_instances_postgres() {
    let Ok(url) = std::env::var("FEAST_TEST_POSTGRES_URL") else { return };
    shared_state(&url).await;
}
