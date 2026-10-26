//! Two diners must never take the last table in a slot, even when several API
//! instances (separate connection pools) book the same slot at the same moment.
//!
//! Runs on SQLite always, and on Postgres when FEAST_TEST_POSTGRES_URL is set:
//!   FEAST_TEST_POSTGRES_URL=postgres://feast:feast@127.0.0.1:5433/feast_test cargo test -p feast-infra --test slot_locking

use feast_application::ports::SlotRoom;
use feast_domain::model::*;
use feast_infra::database::{connect, DatabaseConfig, Repositories};

fn unique() -> String {
    format!("{}{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

const INSTANCES: usize = 3;
const DINERS: usize = 24;
const TABLES: i64 = 5;

fn booking(n: usize, restaurant_id: &str, date: &str, created_at: &str) -> Booking {
    Booking {
        id: format!("bk_{restaurant_id}_{n}"),
        user_id: format!("u_diner_{n}"),
        code: format!("FST-{n:04}"),
        restaurant: RestaurantRef {
            id: restaurant_id.into(),
            slug: restaurant_id.into(),
            name: "Lock Test".into(),
            area: "Test".into(),
            art: Art { hue: 10, motif: CuisineMotif::Curry, image: None, image_credit: None },
        },
        date: date.into(),
        time: "19:30".into(),
        guests: 2,
        deal: None,
        special_request: None,
        contact: Contact { name: "D".into(), phone: "0400000000".into() },
        status: BookingStatus::PendingPayment,
        cover_charge: 0,
        currency: "AUD".into(),
        payment: None,
        created_at: created_at.into(),
        checked_in_at: None,
    }
}

/// `INSTANCES` independent pools race `DINERS` bookings for a slot with `TABLES` tables.
async fn race(url: &str) {
    let mut instances: Vec<Repositories> = Vec::new();
    for _ in 0..INSTANCES {
        instances.push(connect(&DatabaseConfig { url: url.into(), max_connections: 8 }).await.unwrap());
    }
    // Unique restaurant per run so the test can reuse a database.
    let restaurant_id = format!("r_lock_{}", unique());
    let date = "2031-01-15";
    let now = chrono::Utc::now();
    let created_at = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let room = SlotRoom { tables: TABLES, pending_since: (now - chrono::Duration::minutes(20)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true) };

    let mut tasks = Vec::new();
    for n in 0..DINERS {
        let repos = instances[n % INSTANCES].clone();
        let (b, room, key) = (booking(n, &restaurant_id, date, &created_at), room.clone(), format!("{restaurant_id}-key-{n}"));
        tasks.push(tokio::spawn(async move { repos.bookings.insert_if_room(&b, &key, &[], &room).await }));
    }
    let mut won = 0;
    for t in tasks {
        if t.await.unwrap().unwrap() {
            won += 1;
        }
    }
    assert_eq!(won, TABLES as usize, "exactly one diner per table");

    let held = instances[0].bookings.tables_held(&restaurant_id, date, &room.pending_since).await.unwrap();
    assert_eq!(held.get("19:30"), Some(&TABLES));

    // An expired payment hold frees its table again.
    let later = SlotRoom { tables: TABLES, pending_since: (now + chrono::Duration::minutes(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true) };
    let b = booking(DINERS, &restaurant_id, date, &created_at);
    assert!(
        instances[1].bookings.insert_if_room(&b, &format!("{restaurant_id}-key-late"), &[], &later).await.unwrap(),
        "expired holds no longer block the slot"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn sqlite_never_overbooks_a_slot() {
    let dir = std::env::temp_dir().join(format!("feast-lock-{}", unique()));
    std::fs::create_dir_all(&dir).unwrap();
    race(&format!("sqlite://{}", dir.join("feast.db").display())).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn postgres_never_overbooks_a_slot() {
    match std::env::var("FEAST_TEST_POSTGRES_URL") {
        Ok(url) if !url.is_empty() => race(&url).await,
        _ => eprintln!("skipped: set FEAST_TEST_POSTGRES_URL to run against Postgres"),
    }
}
