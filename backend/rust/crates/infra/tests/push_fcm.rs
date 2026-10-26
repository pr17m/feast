//! FCM HTTP v1 adapter against a fake Google (no network): OAuth2 JWT-bearer
//! token exchange, message send, and UNREGISTERED tokens reported as such.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use feast_application::ports::PushSender;
use feast_domain::push::{PushApp, PushMessage, PushOutcome, PushPlatform, PushToken};
use feast_infra::push::{FcmPushSender, ServiceAccount};

fn fake_google() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(vec![]));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut s = stream.unwrap();
            let mut buf = vec![0u8; 16384];
            let mut req = String::new();
            // Read headers + body (small requests; loop until the body is in).
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
            log.lock().unwrap().push(req.clone());
            let (status, body) = if req.starts_with("POST /token") {
                ("200 OK", r#"{"access_token":"ya29.fake","expires_in":3600,"token_type":"Bearer"}"#)
            } else if req.contains("\"token\":\"gone") {
                (
                    "404 Not Found",
                    r#"{"error":{"code":404,"status":"NOT_FOUND","details":[{"@type":"type.googleapis.com/google.firebase.fcm.v1.FcmError","errorCode":"UNREGISTERED"}]}}"#,
                )
            } else {
                ("200 OK", r#"{"name":"projects/feast-test/messages/1"}"#)
            };
            let _ = write!(s, "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
        }
    });
    (addr, seen)
}

fn token(t: &str, platform: PushPlatform) -> PushToken {
    PushToken { token: t.into(), user_id: "u_1".into(), platform, app: PushApp::Diner, created_at: String::new(), last_seen_at: String::new() }
}

#[tokio::test]
async fn sends_with_service_account_and_detects_unregistered() {
    let (base, seen) = fake_google();
    let key = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_rsa.pem")).unwrap();
    let json = serde_json::json!({
        "type": "service_account",
        "project_id": "feast-test",
        "client_email": "push@feast-test.iam.gserviceaccount.com",
        "private_key": key,
        "token_uri": format!("{base}/token"),
    })
    .to_string();
    let account = ServiceAccount::load(&json, "").unwrap();
    let fcm = FcmPushSender::new(account, Some(&base)).unwrap();
    let msg = PushMessage {
        title: "Booking confirmed".into(),
        body: "Table for 2".into(),
        link: Some("/bookings/bk_1".into()),
        notification_id: "ntf_1".into(),
        kind: "BOOKING_CREATED".into(),
    };

    assert_eq!(fcm.send(&token("good_token_1234567890", PushPlatform::Android), &msg).await, PushOutcome::Sent);
    assert_eq!(fcm.send(&token("good_token_ios_123456", PushPlatform::Ios), &msg).await, PushOutcome::Sent);
    assert_eq!(fcm.send(&token("gone_token_1234567890", PushPlatform::Android), &msg).await, PushOutcome::Unregistered);

    let reqs = seen.lock().unwrap().clone();
    let token_calls: Vec<_> = reqs.iter().filter(|r| r.starts_with("POST /token")).collect();
    assert_eq!(token_calls.len(), 1, "access token is cached");
    assert!(token_calls[0].contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer"));
    assert!(token_calls[0].contains("assertion=eyJ"), "signed JWT assertion");
    let sends: Vec<_> = reqs.iter().filter(|r| r.starts_with("POST /v1/projects/feast-test/messages:send")).collect();
    assert_eq!(sends.len(), 3);
    assert!(sends[0].to_lowercase().contains("authorization: bearer ya29.fake"));
    assert!(sends[0].contains("\"link\":\"/bookings/bk_1\""));
    assert!(sends[1].contains("\"apns\""));
}

#[test]
fn rejects_missing_or_bad_credentials() {
    assert!(ServiceAccount::load("", "").is_err());
    assert!(ServiceAccount::load("{\"nope\":1}", "").is_err());
    let acct = ServiceAccount::load(r#"{"project_id":"p","client_email":"e","private_key":"not a key"}"#, "").unwrap();
    assert!(FcmPushSender::new(acct, None).is_err());
}
