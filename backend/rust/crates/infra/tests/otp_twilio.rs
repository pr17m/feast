//! Twilio Verify adapter against a fake Twilio (no network).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use feast_application::ports::OtpService;
use feast_infra::otp::TwilioVerifyOtpService;

/// Minimal HTTP server: records request lines + bodies, answers by path.
fn fake_twilio() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(vec![]));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut s = stream.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = s.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            log.lock().unwrap().push(req.clone());
            let body = if req.contains("VerificationCheck") {
                if req.contains("Code=123456") {
                    r#"{"status":"approved"}"#
                } else {
                    r#"{"status":"pending"}"#
                }
            } else {
                r#"{"status":"pending","sid":"VE1"}"#
            };
            let _ = write!(s, "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
        }
    });
    (addr, seen)
}

#[tokio::test]
async fn sends_and_checks_codes() {
    let (base, seen) = fake_twilio();
    let otp = TwilioVerifyOtpService::new("AC1", "tok", "VA1", Some(&base)).unwrap();
    assert_eq!(otp.send("+61412345678").await.unwrap(), None, "real SMS: no hint");
    assert!(otp.verify("+61412345678", "123456").await.unwrap());
    assert!(!otp.verify("+61412345678", "000000").await.unwrap());
    let reqs = seen.lock().unwrap().join("\n");
    assert!(reqs.contains("POST /v2/Services/VA1/Verifications"));
    assert!(reqs.contains("To=%2B61412345678"));
    assert!(reqs.contains("Channel=sms"));
    assert!(reqs.to_lowercase().contains("authorization: basic"));
}
