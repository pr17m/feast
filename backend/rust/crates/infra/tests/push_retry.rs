//! Push delivery through FCM (fake Google, no network) with several workers on
//! one database: each phone gets a notification once; a passing FCM failure
//! (503) is retried with backoff and sent once it recovers; a phone that keeps
//! failing is given up after `PUSH_MAX_ATTEMPTS`; uninstalled apps' tokens are
//! dropped.
//!
//! Runs on SQLite always, and on Postgres when FEAST_TEST_POSTGRES_URL is set.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use feast_application::ports::PushSender;
use feast_application::push::{push_to_phones, retry_due_pushes};
use feast_domain::model::{Notification, NotificationKind, Role, User, UserStatus};
use feast_domain::push::{push_backoff_ms, PushApp, PushPlatform, PushToken, PUSH_MAX_ATTEMPTS};
use feast_infra::database::{connect, DatabaseConfig, Repositories};
use feast_infra::push::{FcmPushSender, ServiceAccount};

fn unique() -> String {
    format!("{}{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())
}

type Sends = Arc<Mutex<HashMap<String, Vec<u16>>>>;

/// Fake Google: OAuth token endpoint + FCM send. Per token: `flaky…` answers
/// 503 the first time then 200, `down…` always 503, `gone…` 404 UNREGISTERED,
/// anything else 200. Records each send's status per token.
fn fake_google() -> (String, Sends) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let sends: Sends = Arc::new(Mutex::new(HashMap::new()));
    let log = sends.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut s = stream.unwrap();
            let mut buf = vec![0u8; 16384];
            let mut req = String::new();
            loop {
                let n = s.read(&mut buf).unwrap();
                req.push_str(&String::from_utf8_lossy(&buf[..n]));
                if let Some(i) = req.find("\r\n\r\n") {
                    let len = req[..i]
                        .lines()
                        .find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                        .unwrap_or(0);
                    if req.len() >= i + 4 + len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let (status, body): (u16, &str) = if req.starts_with("POST /token") {
                (200, r#"{"access_token":"ya29.fake","expires_in":3600,"token_type":"Bearer"}"#)
            } else {
                let tok = req.split("\"token\":\"").nth(1).and_then(|r| r.split('"').next()).unwrap_or_default().to_owned();
                let mut log = log.lock().unwrap();
                let seen = log.entry(tok.clone()).or_default();
                let status = if tok.starts_with("gone") {
                    404
                } else if tok.starts_with("down") || (tok.starts_with("flaky") && seen.is_empty()) {
                    503
                } else {
                    200
                };
                seen.push(status);
                match status {
                    404 => (404, r#"{"error":{"code":404,"status":"NOT_FOUND","details":[{"errorCode":"UNREGISTERED"}]}}"#),
                    503 => (503, r#"{"error":{"code":503,"status":"UNAVAILABLE"}}"#),
                    _ => (200, r#"{"name":"projects/feast-test/messages/1"}"#),
                }
            };
            let _ = write!(s, "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
        }
    });
    (addr, sends)
}

fn fcm(base: &str) -> FcmPushSender {
    let key = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_rsa.pem")).unwrap();
    let json = serde_json::json!({
        "type": "service_account", "project_id": "feast-test", "client_email": "push@feast-test.iam.gserviceaccount.com",
        "private_key": key, "token_uri": format!("{base}/token"),
    })
    .to_string();
    FcmPushSender::new(ServiceAccount::load(&json, "").unwrap(), Some(base)).unwrap()
}

fn notification(user: &str) -> Notification {
    Notification {
        id: format!("ntf_{}", unique()),
        user_id: user.into(),
        restaurant_id: None,
        kind: NotificationKind::PaymentReceived,
        title: "Refund of $30 sent".into(),
        body: "It's on its way back to your card.".into(),
        link: Some("/bookings".into()),
        event_id: format!("evt_{}", unique()),
        created_at: "2031-01-01T00:00:00Z".into(),
        read_at: None,
    }
}

async fn phone(r: &Repositories, user: &str, token: &str) {
    let t = PushToken {
        token: token.into(),
        user_id: user.into(),
        platform: PushPlatform::Android,
        app: PushApp::Diner,
        created_at: "2031-01-01T00:00:00Z".into(),
        last_seen_at: "2031-01-01T00:00:00Z".into(),
    };
    r.push_tokens.upsert_token(&t).await.unwrap();
}

async fn retries(url: &str) {
    let (base, sends) = fake_google();
    let workers: Vec<(Repositories, Arc<dyn PushSender>)> = {
        let mut v = Vec::new();
        for _ in 0..3 {
            let repos = connect(&DatabaseConfig { url: url.into(), max_connections: 8 }).await.unwrap();
            v.push((repos, Arc::new(fcm(&base)) as Arc<dyn PushSender>));
        }
        v
    };
    let tag = unique();
    let user = format!("u_push_{tag}");
    let (good, flaky, down, gone) = (format!("good_{tag}"), format!("flaky_{tag}"), format!("down_{tag}"), format!("gone_{tag}"));
    let diner = User {
        id: user.clone(),
        name: "Pia Push".into(),
        phone: Some(format!("+614{}", &tag[tag.len() - 8..])),
        email: None,
        role: Role::Customer,
        restaurant_ids: vec![],
        status: UserStatus::Active,
        created_at: "2031-01-01T00:00:00Z".into(),
    };
    workers[0].0.users.insert(&diner, None).await.unwrap();
    for t in [&good, &flaky, &down, &gone] {
        phone(&workers[0].0, &user, t).await;
    }
    let n = notification(&user);
    let t0 = 1_000_000_000_000i64;

    // Three workers handle the same notification.created at once.
    let mut tasks = Vec::new();
    for (repos, sender) in &workers {
        let (repo, sender, n) = (repos.push_tokens.clone(), sender.clone(), n.clone());
        tasks.push(tokio::spawn(async move { push_to_phones(repo.as_ref(), sender.as_ref(), &n, t0).await.unwrap() }));
    }
    let mut sent = 0;
    for t in tasks {
        sent += t.await.unwrap();
    }
    assert_eq!(sent, 1, "only the healthy phone got it on the first try");
    let count = |t: &str| sends.lock().unwrap().get(t).cloned().unwrap_or_default();
    assert_eq!(count(&good), vec![200], "sent once across workers");
    assert_eq!(count(&flaky), vec![503]);
    assert_eq!(count(&gone), vec![404]);
    assert!(workers[0].0.push_tokens.tokens_for(&user).await.unwrap().iter().all(|t| t.token != gone), "uninstalled app's token dropped");

    // Not due yet: nothing is retried.
    for (repos, sender) in &workers {
        assert_eq!(retry_due_pushes(repos.push_tokens.as_ref(), sender.as_ref(), t0 + 1_000, 50).await.unwrap(), 0);
    }
    assert_eq!(count(&flaky).len(), 1);

    // After the backoff, three sweepers race: the flaky phone gets it exactly once.
    let mut now = t0 + push_backoff_ms(1);
    let mut tasks = Vec::new();
    for (repos, sender) in &workers {
        let (repo, sender) = (repos.push_tokens.clone(), sender.clone());
        tasks.push(tokio::spawn(async move { retry_due_pushes(repo.as_ref(), sender.as_ref(), now, 50).await.unwrap() }));
    }
    let mut resent = 0;
    for t in tasks {
        resent += t.await.unwrap();
    }
    assert_eq!(resent, 1);
    assert_eq!(count(&flaky), vec![503, 200], "retried once, then never again");
    assert_eq!(count(&good), vec![200], "a sent push is never repeated");

    // The phone that stays down is tried PUSH_MAX_ATTEMPTS times, then given up.
    for attempt in 2..=PUSH_MAX_ATTEMPTS + 2 {
        now += push_backoff_ms(attempt);
        retry_due_pushes(workers[(attempt % 3) as usize].0.push_tokens.as_ref(), workers[0].1.as_ref(), now, 50).await.unwrap();
    }
    assert_eq!(count(&down).len() as i64, PUSH_MAX_ATTEMPTS, "gave up after the last attempt");
    assert_eq!(count(&flaky).len(), 2);
    assert_eq!(count(&good).len(), 1);

    // A worker that died mid-send: its claim expires and the push is sent once.
    let n2 = notification(&user);
    assert!(workers[1].0.push_tokens.claim_push(&n2, &good, now + 60_000).await.unwrap(), "claimed, then the worker crashed");
    let (repo, sender) = (workers[2].0.push_tokens.as_ref(), workers[2].1.as_ref());
    assert_eq!(retry_due_pushes(repo, sender, now + 1_000, 50).await.unwrap(), 0, "claim still live");
    assert_eq!(retry_due_pushes(repo, sender, now + 61_000, 50).await.unwrap(), 1, "claim expired: sent");
    assert_eq!(count(&good).len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_pushes_are_retried_once_sqlite() {
    let dir = std::env::temp_dir().join(format!("feast-push-{}", unique()));
    std::fs::create_dir_all(&dir).unwrap();
    retries(&format!("sqlite://{}", dir.join("push.db").display())).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_pushes_are_retried_once_postgres() {
    let Ok(url) = std::env::var("FEAST_TEST_POSTGRES_URL") else { return };
    retries(&url).await;
}
