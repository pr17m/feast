//! A restaurant's daily deal limit must hold even when several API instances
//! (separate connection pools) take payments for discounted bills at once.
//!
//! Runs on SQLite always, and on Postgres when FEAST_TEST_POSTGRES_URL is set:
//!   FEAST_TEST_POSTGRES_URL=postgres://feast:feast@127.0.0.1:5433/feast_test cargo test -p feast-infra --test deal_budget

use feast_application::ports::DealClaim;
use feast_domain::deals::ClaimOutcome;
use feast_infra::database::{connect, DatabaseConfig, Repositories};

fn unique() -> String {
    format!("{}{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

fn iso(t: chrono::DateTime<chrono::Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

const INSTANCES: usize = 3;
const BILLS: usize = 24;
const LIMIT: i64 = 5;

fn claim(restaurant_id: &str, n: usize, at: &str) -> DealClaim {
    DealClaim {
        bill_id: format!("bill_{restaurant_id}_{n}"),
        restaurant_id: restaurant_id.into(),
        date: "2031-01-15".into(),
        intent_id: format!("pi_{restaurant_id}_{n}"),
        claimed_at: at.into(),
        booking_id: None,
        pending_offline: false,
    }
}

async fn race(url: &str) {
    let mut instances: Vec<Repositories> = Vec::new();
    for _ in 0..INSTANCES {
        instances.push(connect(&DatabaseConfig { url: url.into(), max_connections: 8 }).await.unwrap());
    }
    let restaurant_id = format!("r_deals_{}", unique());
    let now = chrono::Utc::now();
    let at = iso(now);
    let held_since = iso(now - chrono::Duration::minutes(20));

    let mut tasks = Vec::new();
    for n in 0..BILLS {
        let repos = instances[n % INSTANCES].clone();
        let (c, since) = (claim(&restaurant_id, n, &at), held_since.clone());
        tasks.push(tokio::spawn(async move { repos.billing.claim_deal(&c, Some(LIMIT), &since, &[]).await }));
    }
    let mut won = Vec::new();
    for (n, t) in tasks.into_iter().enumerate() {
        if t.await.unwrap().unwrap() == ClaimOutcome::Claimed {
            won.push(n);
        }
    }
    assert_eq!(won.len(), LIMIT as usize, "exactly the daily limit of deals is handed out");
    let repos = &instances[0];
    assert_eq!(repos.billing.deals_used(&restaurant_id, "2031-01-15", &held_since).await.unwrap(), LIMIT);

    // Re-claiming a bill that already holds a deal (a retried payment) is fine.
    assert_eq!(repos.billing.claim_deal(&claim(&restaurant_id, won[0], &at), Some(LIMIT), &held_since, &[]).await.unwrap(), ClaimOutcome::Claimed);

    // A failed payment gives its deal back, so the next bill can have it...
    let loser = (0..BILLS).find(|n| !won.contains(n)).unwrap();
    let mut first = claim(&restaurant_id, won[0], &at);
    repos.billing.release_deal(&first.bill_id, "pi_someone_else", &[]).await.unwrap();
    assert_eq!(
        repos.billing.claim_deal(&claim(&restaurant_id, loser, &at), Some(LIMIT), &held_since, &[]).await.unwrap(),
        ClaimOutcome::LimitReached,
        "only the holding intent can release"
    );
    repos.billing.release_deal(&first.bill_id, &first.intent_id, &[]).await.unwrap();
    assert_eq!(repos.billing.deals_used(&restaurant_id, "2031-01-15", &held_since).await.unwrap(), LIMIT - 1);
    assert_eq!(repos.billing.claim_deal(&claim(&restaurant_id, loser, &at), Some(LIMIT), &held_since, &[]).await.unwrap(), ClaimOutcome::Claimed);

    // ...and holds from abandoned checkouts stop counting after the hold window.
    let later = iso(now + chrono::Duration::minutes(1));
    assert_eq!(repos.billing.deals_used(&restaurant_id, "2031-01-15", &later).await.unwrap(), 0);
    first.intent_id = "pi_retry".into();
    first.claimed_at = iso(now + chrono::Duration::minutes(2));
    assert_eq!(repos.billing.claim_deal(&first, Some(LIMIT), &later, &[]).await.unwrap(), ClaimOutcome::Claimed);

    // No limit: claims are still recorded (for the owner's "deals used today").
    let other = format!("{restaurant_id}_nolimit");
    for n in 0..3 {
        assert_eq!(repos.billing.claim_deal(&claim(&other, n, &at), None, &held_since, &[]).await.unwrap(), ClaimOutcome::Claimed);
    }
    assert_eq!(repos.billing.deals_used(&other, "2031-01-15", &held_since).await.unwrap(), 3);

    // One discounted bill per booking: several bills for the same booking,
    // paid at once on different instances, get exactly one deal between them.
    let booked = format!("{restaurant_id}_booked");
    let booking_id = format!("bk_{booked}");
    let mut tasks = Vec::new();
    for n in 0..6 {
        let repos = instances[n % INSTANCES].clone();
        let mut c = claim(&booked, n, &at);
        c.booking_id = Some(booking_id.clone());
        let since = held_since.clone();
        tasks.push(tokio::spawn(async move { repos.billing.claim_deal(&c, None, &since, &[]).await }));
    }
    let mut outcomes = Vec::new();
    for t in tasks {
        outcomes.push(t.await.unwrap().unwrap());
    }
    assert_eq!(outcomes.iter().filter(|o| **o == ClaimOutcome::Claimed).count(), 1, "{outcomes:?}");
    assert!(outcomes.iter().all(|o| matches!(o, ClaimOutcome::Claimed | ClaimOutcome::BookingDealUsed)));
    assert!(!repos.billing.booking_deal_used(&booking_id).await.unwrap(), "held, not yet paid");
    // An abandoned checkout's hold expires: a new bill for the booking can have the deal.
    let mut retry = claim(&booked, 99, &iso(now + chrono::Duration::minutes(2)));
    retry.booking_id = Some(booking_id.clone());
    assert_eq!(repos.billing.claim_deal(&retry, None, &later, &[]).await.unwrap(), ClaimOutcome::Claimed);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn daily_limit_holds_under_concurrency_sqlite() {
    let dir = std::env::temp_dir().join(format!("feast-deals-{}", unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("deals.db").display());
    race(&url).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn daily_limit_holds_under_concurrency_postgres() {
    let Ok(url) = std::env::var("FEAST_TEST_POSTGRES_URL") else {
        eprintln!("FEAST_TEST_POSTGRES_URL not set; skipping the Postgres run");
        return;
    };
    race(&url).await;
}
