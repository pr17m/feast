//! SQLite adapter against the real schema: seed, query, transactional outbox, idempotency.

use std::sync::Arc;

use feast_application::ports::*;
use feast_domain::events::DomainEvent;
use feast_domain::model::*;
use feast_domain::EventEnvelope;
use feast_infra::auth::Argon2Hasher;
use feast_infra::clock::SystemClock;
use feast_infra::database::{connect, DatabaseConfig, Repositories};
use feast_infra::seed::{CountryFile, DemoReset, DemoSeed, Seeder};
use feast_infra::storage::LocalDiskStorage;

fn countries() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../config/countries")
}

fn seeder(repos: &Repositories, clock: Arc<dyn Clock>, cc: &str, with_history: bool) -> Seeder {
    Seeder {
        repos: repos.clone(),
        passwords: Arc::new(Argon2Hasher::default()),
        clock,
        files: Arc::new(LocalDiskStorage::new(std::env::temp_dir().join(format!("feast-test-files-{}", std::process::id())))),
        country: CountryFile::load(&countries(), cc).unwrap(),
        seed: DemoSeed::bundled(),
        with_history,
    }
}

async fn seeded_country(cc: &str, tz: &str) -> (Repositories, Arc<dyn Clock>) {
    let repos = connect(&DatabaseConfig { url: "sqlite::memory:".into(), max_connections: 4 }).await.unwrap();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::from_name(tz).unwrap());
    seeder(&repos, clock.clone(), cc, true).run().await.unwrap();
    (repos, clock)
}

async fn seeded() -> (Repositories, Arc<dyn Clock>) {
    seeded_country("in", "Asia/Kolkata").await
}

#[tokio::test]
async fn every_country_file_is_valid() {
    for cc in ["in", "au", "nz", "ca", "gb"] {
        let f = CountryFile::load(&countries(), cc).unwrap_or_else(|e| panic!("{cc}: {e}"));
        assert_eq!(f.market.country_code, cc.to_uppercase());
    }
}

#[tokio::test]
async fn australia_seeds_only_australia() {
    let (repos, _) = seeded_country("au", "Australia/Sydney").await;
    let all = repos.restaurants.list().await.unwrap();
    assert_eq!(all.len(), 6);
    assert!(all.iter().all(|r| r.detail.country_code == "AU" && r.detail.currency == "AUD"));
    assert_eq!(repos.markets.list_markets().await.unwrap().len(), 1);
    assert!(repos.markets.list_cities().await.unwrap().iter().all(|c| c.country_code == "AU"));
    let users = repos.users.list().await.unwrap();
    assert!(users.iter().all(|u| u.phone.as_deref().is_none_or(|p| p.starts_with("+61"))));
    // Every listed restaurant has its documents approved and a subscription.
    let docs = repos.compliance.documents_for("r_harbour_vine").await.unwrap();
    assert_eq!(docs.len(), 4);
    assert!(repos.subscriptions.get_record("r_harbour_vine").await.unwrap().is_some());
    assert_eq!(repos.subscriptions.payments_for("r_harbour_vine").await.unwrap().len(), 2);
}

#[tokio::test]
async fn seed_and_read_back() {
    let (repos, clock) = seeded().await;
    let all = repos.restaurants.list().await.unwrap();
    assert_eq!(all.len(), 14);
    let oe = repos.restaurants.get_by_slug("olive-and-ember").await.unwrap().unwrap();
    assert!(!oe.menu.is_empty());
    // The printed restaurant QR resolves by short code, case-insensitively.
    let r = repos.restaurants.find_by_short_code(&oe.short_code.to_lowercase()).await.unwrap().unwrap();
    assert_eq!(r.detail.id, oe.detail.id);

    let owners = repos.users.owners_of("r_olive_ember").await.unwrap();
    assert_eq!(owners.len(), 1);
    assert_eq!(owners[0].restaurant_ids, vec!["r_hops_republic".to_string(), "r_olive_ember".to_string()]);
    let (_, hash) = repos.users.find_by_email("owner@feast.demo").await.unwrap().unwrap();
    assert!(hash.is_some(), "staff get a password");

    let week = repos.bookings.list_by_restaurant_between("r_olive_ember", &clock.local_date("", -7), &clock.local_date("", 0)).await.unwrap();
    assert!(week.len() >= 8);
    assert!(!repos.billing.payments_by_restaurant("r_olive_ember").await.unwrap().is_empty());
    assert!(!repos.admin.is_empty().await.unwrap());
}

#[tokio::test]
async fn booking_insert_is_idempotent_and_writes_outbox_atomically() {
    let (repos, clock) = seeded().await;
    let oe = repos.restaurants.get("r_olive_ember").await.unwrap().unwrap();
    let b = Booking {
        id: "bk_test".into(),
        user_id: "u_kavya".into(),
        code: "FST-TEST".into(),
        restaurant: oe.detail.reference(),
        date: clock.local_date("", 1),
        time: "20:00".into(),
        guests: 2,
        deal: None,
        special_request: None,
        contact: Contact { name: "K".into(), phone: "9876543210".into() },
        status: BookingStatus::Confirmed,
        cover_charge: 0,
        currency: "INR".into(),
        payment: None,
        created_at: clock.now_iso(),
        checked_in_at: None,
    };
    let ev = EventEnvelope::new(
        "evt_1".into(),
        clock.now_iso(),
        Some(oe.detail.id.clone()),
        Some("u_kavya".into()),
        DomainEvent::BookingCreated { booking: b.clone() },
    );
    repos.bookings.insert(&b, "idem-1", std::slice::from_ref(&ev)).await.unwrap();

    // Same idempotency key → conflict, and no second outbox row.
    let mut b2 = b.clone();
    b2.id = "bk_test2".into();
    let ev2 = EventEnvelope::new("evt_2".into(), clock.now_iso(), None, None, DomainEvent::BookingCreated { booking: b2.clone() });
    let err = repos.bookings.insert(&b2, "idem-1", &[ev2]).await.unwrap_err();
    assert_eq!(err.code, "DUPLICATE_REQUEST");
    assert_eq!(repos.bookings.find_by_idempotency_key("u_kavya", "idem-1").await.unwrap().unwrap().id, "bk_test");

    let pending = repos.outbox.fetch_pending(10).await.unwrap();
    assert_eq!(pending.len(), 1, "rolled-back insert left no event");
    assert_eq!(pending[0].1, ev);
    repos.outbox.mark_published(&[pending[0].0]).await.unwrap();
    assert!(repos.outbox.fetch_pending(10).await.unwrap().is_empty());

    let held = repos.bookings.tables_held("r_olive_ember", &clock.local_date("", 1), "1970-01-01T00:00:00Z").await.unwrap();
    assert_eq!(held.get("20:00"), Some(&1));
}

#[tokio::test]
async fn demo_reset_restores_seed() {
    let (repos, clock) = seeded().await;
    let reset = DemoReset { seeder: seeder(&repos, clock, "in", false) };
    let mut r = repos.restaurants.get("r_olive_ember").await.unwrap().unwrap();
    r.detail.name = "Changed".into();
    repos.restaurants.update(&r).await.unwrap();
    reset.reset().await.unwrap();
    assert_eq!(repos.restaurants.get("r_olive_ember").await.unwrap().unwrap().detail.name, "Olive & Ember");
}
