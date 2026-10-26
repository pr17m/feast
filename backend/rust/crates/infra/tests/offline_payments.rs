//! A bill can be paid once, even when the diner's card payment completes at the
//! same moment the restaurant confirms their counter payment — on separate API
//! instances (separate connection pools). Also: one open counter payment per
//! bill, decisions are final, and every step lands in the audit trail.
//!
//! Runs on SQLite always, and on Postgres when FEAST_TEST_POSTGRES_URL is set:
//!   FEAST_TEST_POSTGRES_URL=postgres://feast:feast@127.0.0.1:5433/feast_test cargo test -p feast-infra --test offline_payments

use feast_application::ports::Settled;
use feast_domain::audit::{Actor, ActorKind, AuditQuery};
use feast_domain::model::*;
use feast_domain::{DomainEvent, EventEnvelope};
use feast_infra::database::{connect, DatabaseConfig};

fn unique() -> String {
    format!("{}{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn restaurant(id: &str) -> RestaurantRef {
    RestaurantRef {
        id: id.into(),
        slug: id.into(),
        name: "Test".into(),
        area: "CBD".into(),
        art: Art { hue: 1, motif: CuisineMotif::Cafe, image: None, image_credit: None },
    }
}

fn bill(id: &str, rid: &str) -> Bill {
    Bill {
        id: id.into(),
        restaurant_id: rid.into(),
        currency: "AUD".into(),
        bill_amount: 5000,
        lines: vec![],
        applied_offer: None,
        savings: 0,
        payable: 5000,
        deal_notice: None,
        booking_id: None,
    }
}

fn intent(id: &str, bill_id: &str, provider: &str) -> PaymentIntent {
    PaymentIntent {
        id: id.into(),
        purpose: PaymentPurpose::Bill,
        reference_id: bill_id.into(),
        amount: 5000,
        currency: "AUD".into(),
        provider: provider.into(),
        checkout: None,
        provider_ref: None,
        plan_id: None,
        user_id: Some("u_diner".into()),
        status: PaymentStatus::Created,
        result_ref: None,
        failure_reason: None,
        provider_payment_id: None,
    }
}

fn paid(b: &Bill, i: &PaymentIntent, bp_id: &str, method: PaymentMethod, settlement: Settlement) -> (PaymentIntent, BillPayment) {
    let mut i = i.clone();
    i.status = PaymentStatus::Succeeded;
    i.result_ref = Some(bp_id.into());
    let bp = BillPayment {
        id: bp_id.into(),
        user_id: "u_diner".into(),
        bill: b.clone(),
        restaurant: restaurant(&b.restaurant_id),
        payment: PaymentRef { intent_id: i.id.clone(), method, amount: 5000, currency: "AUD".into(), provider: i.provider.clone(), paid_at: now(), settlement },
    };
    (i, bp)
}

fn offline(id: &str, b: &Bill, intent_id: &str) -> OfflinePayment {
    OfflinePayment {
        id: id.into(),
        bill_id: b.id.clone(),
        intent_id: intent_id.into(),
        restaurant: restaurant(&b.restaurant_id),
        user_id: "u_diner".into(),
        diner_name: "Dee".into(),
        method: PaymentMethod::Cash,
        amount: 5000,
        currency: "AUD".into(),
        savings: 0,
        booking_id: None,
        status: OfflinePaymentStatus::PendingApproval,
        submitted_at: now(),
        expires_at: now(),
        decided_at: None,
        decided_by: None,
        reason: None,
        amount_received: None,
        bill_payment_id: None,
    }
}

fn ev(rid: &str, event: DomainEvent, actor: Actor) -> EventEnvelope {
    EventEnvelope::new(format!("evt_{}", unique()), now(), Some(rid.into()), Some("u_diner".into()), event).by(actor)
}

const BILLS: usize = 12;

async fn race(url: &str) {
    let a = connect(&DatabaseConfig { url: url.into(), max_connections: 8 }).await.unwrap();
    let b = connect(&DatabaseConfig { url: url.into(), max_connections: 8 }).await.unwrap();
    let rid = format!("r_off_{}", unique());
    let owner = Actor { kind: ActorKind::User, user_id: Some("u_owner".into()), role: Some(Role::Owner), ..Actor::default() };

    let mut tasks = Vec::new();
    for n in 0..BILLS {
        let bl = bill(&format!("bill_{rid}_{n}"), &rid);
        a.billing.insert_bill(&bl).await.unwrap();
        let on = intent(&format!("pi_on_{rid}_{n}"), &bl.id, "mock");
        a.billing.insert_intent(&on, &[]).await.unwrap();
        let off = intent(&format!("pi_off_{rid}_{n}"), &bl.id, "offline");
        let op = offline(&format!("op_{rid}_{n}"), &bl, &off.id);
        let submitted = ev(&rid, DomainEvent::OfflinePaymentSubmitted { payment: op.clone() }, owner.clone());
        a.offline.insert_offline(&op, &off, &[submitted]).await.unwrap();

        let (on_paid, bp_on) = paid(&bl, &on, &format!("bp_on_{rid}_{n}"), PaymentMethod::Card, Settlement::Online);
        let (off_paid, bp_off) = paid(&bl, &off, &format!("bp_off_{rid}_{n}"), PaymentMethod::Cash, Settlement::Offline);
        let mut approved = op.clone();
        approved.status = OfflinePaymentStatus::Approved;
        approved.bill_payment_id = Some(bp_off.id.clone());
        let approve_ev = ev(&rid, DomainEvent::OfflinePaymentApproved { payment: approved.clone() }, owner.clone());
        let (ra, rb) = (a.clone(), b.clone());
        // Alternate which instance takes which side.
        let (online_repo, offline_repo) = if n % 2 == 0 { (ra, rb) } else { (rb, ra) };
        tasks.push(tokio::spawn(async move {
            let online = tokio::spawn(async move { online_repo.billing.complete_bill_payment(&on_paid, &bp_on, &[]).await });
            let counter = tokio::spawn(async move { offline_repo.offline.approve_offline(&approved, &off_paid, &bp_off, &[approve_ev]).await });
            (online.await.unwrap(), counter.await.unwrap())
        }));
    }
    let mut online_wins = 0;
    let mut offline_wins = 0;
    for t in tasks {
        let (online, counter) = t.await.unwrap();
        let online_ok = matches!(online, Ok(Settled::Applied));
        let counter_ok = matches!(counter, Ok(true));
        assert!(online_ok ^ counter_ok, "exactly one payment may count: online={online:?} counter={counter:?}");
        if online_ok {
            online_wins += 1;
            assert_eq!(counter.unwrap_err().code, "ALREADY_PAID");
        } else {
            offline_wins += 1;
            // The card payment arrived for a bill already paid at the counter:
            // not applied (the application layer refunds it).
            assert!(matches!(online, Ok(Settled::NotApplied)), "online={online:?}");
        }
    }
    let pays = a.billing.payments_by_restaurant(&rid).await.unwrap();
    assert_eq!(pays.len(), BILLS, "one bill payment per bill");
    assert_eq!(pays.iter().filter(|p| p.payment.settlement == Settlement::Offline).count(), offline_wins);
    eprintln!("online won {online_wins}, counter won {offline_wins}");

    // An approved counter payment is final; a lost one stays pending (never half-applied).
    for n in 0..BILLS {
        let op = a.offline.get_offline(&format!("op_{rid}_{n}")).await.unwrap().unwrap();
        let bp_off = format!("bp_off_{rid}_{n}");
        let won = pays.iter().any(|p| p.id == bp_off);
        assert_eq!(op.status == OfflinePaymentStatus::Approved, won, "status follows the winner");
    }

    // One open counter payment per bill.
    let bl = bill(&format!("bill_{rid}_dup"), &rid);
    a.billing.insert_bill(&bl).await.unwrap();
    let i1 = intent(&format!("pi_dup1_{rid}"), &bl.id, "offline");
    a.offline.insert_offline(&offline(&format!("op_dup1_{rid}"), &bl, &i1.id), &i1, &[]).await.unwrap();
    let i2 = intent(&format!("pi_dup2_{rid}"), &bl.id, "offline");
    let err = a.offline.insert_offline(&offline(&format!("op_dup2_{rid}"), &bl, &i2.id), &i2, &[]).await.unwrap_err();
    assert_eq!(err.code, "OFFLINE_PENDING");

    // Rejecting is final too, and frees the bill for another counter payment.
    let mut rejected = offline(&format!("op_dup1_{rid}"), &bl, &i1.id);
    rejected.status = OfflinePaymentStatus::Rejected;
    rejected.reason = Some("Diner left".into());
    let mut i1f = i1.clone();
    i1f.status = PaymentStatus::Failed;
    let rej_ev = ev(&rid, DomainEvent::OfflinePaymentRejected { payment: rejected.clone() }, owner.clone());
    assert!(a.offline.close_offline(&rejected, &i1f, std::slice::from_ref(&rej_ev)).await.unwrap());
    assert!(!b.offline.close_offline(&rejected, &i1f, &[rej_ev]).await.unwrap(), "already decided");
    a.offline.insert_offline(&offline(&format!("op_dup2_{rid}"), &bl, &i2.id), &i2, &[]).await.unwrap();

    // The audit trail has every committed step with its actor, and nothing from rolled-back ones.
    let page = a.trail.query(&AuditQuery { restaurant_id: Some(rid.clone()), limit: 200, ..AuditQuery::default() }).await.unwrap();
    let count = |action: &str| page.items.iter().filter(|r| r.action == action).count();
    assert_eq!(count("payment.offline_submitted"), BILLS);
    assert_eq!(count("payment.offline_approved"), offline_wins, "a counter approval that lost the race left no audit record");
    assert_eq!(count("payment.offline_rejected"), 1);
    assert!(page.items.iter().all(|r| r.actor.user_id.as_deref() == Some("u_owner")));
    // Paging walks the same records.
    let first = a.trail.query(&AuditQuery { restaurant_id: Some(rid.clone()), limit: 5, ..AuditQuery::default() }).await.unwrap();
    assert_eq!(first.items.len(), 5);
    let second =
        a.trail.query(&AuditQuery { restaurant_id: Some(rid.clone()), limit: 5, cursor: first.next_cursor.clone(), ..AuditQuery::default() }).await.unwrap();
    assert_eq!(second.items[0].id, page.items[5].id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_payment_per_bill_online_vs_counter_sqlite() {
    let dir = std::env::temp_dir().join(format!("feast-offline-{}", unique()));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("offline.db").display());
    race(&url).await;
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_payment_per_bill_online_vs_counter_postgres() {
    let Ok(url) = std::env::var("FEAST_TEST_POSTGRES_URL") else {
        eprintln!("FEAST_TEST_POSTGRES_URL not set; skipping the Postgres run");
        return;
    };
    race(&url).await;
}
