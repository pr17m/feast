//! Social login verification against local fake provider endpoints
//! (a JWKS server for Google/Apple, a fake Graph API for Facebook).

use std::collections::HashMap;
use std::sync::Arc;

use feast_application::ports::IdentityProvider;
use feast_domain::model::{AuthProvider, OAuthLoginRequest};
use feast_infra::oauth::{build, OAuthMode, OAuthProviderConfig};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const PEM: &str = include_str!("fixtures/test_rsa.pem");
const N: &str = include_str!("fixtures/test_rsa.n");

/// Serve fixed JSON bodies by path prefix (query string ignored). Returns the base URL.
async fn serve(routes: HashMap<&'static str, String>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let routes = Arc::new(routes);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let routes = routes.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let path = req.split_whitespace().nth(1).unwrap_or("/").split('?').next().unwrap_or("/").to_owned();
                let (status, body) =
                    routes.iter().find(|(k, _)| path.ends_with(*k)).map(|(_, v)| ("200 OK", v.clone())).unwrap_or(("404 Not Found", "{}".into()));
                let resp = format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}

fn id_token(kid: &str, claims: serde_json::Value) -> String {
    let mut h = Header::new(Algorithm::RS256);
    h.kid = Some(kid.into());
    encode(&h, &claims, &EncodingKey::from_rsa_pem(PEM.as_bytes()).unwrap()).unwrap()
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn cfg(provider: AuthProvider, mode: OAuthMode, ids: &[&str], secret: Option<&str>, endpoint: Option<String>) -> OAuthProviderConfig {
    OAuthProviderConfig {
        provider,
        mode,
        client_ids: ids.iter().map(|s| s.to_string()).collect(),
        client_secret: secret.map(str::to_owned),
        redirect_uri: None,
        trust_email: true,
        endpoint_override: endpoint,
    }
}

fn one(c: OAuthProviderConfig) -> Arc<dyn IdentityProvider> {
    build(&[c]).unwrap().remove(0)
}

#[tokio::test]
async fn google_id_token_is_verified() {
    let jwks = json!({ "keys": [{ "kid": "k1", "kty": "RSA", "alg": "RS256", "use": "sig", "n": N.trim(), "e": "AQAB" }] }).to_string();
    let base = serve(HashMap::from([("/certs", jwks)])).await;
    let g = one(cfg(AuthProvider::Google, OAuthMode::Live, &["web-client", "android-client"], None, Some(format!("{base}/certs"))));
    let good = json!({ "iss": "https://accounts.google.com", "aud": "android-client", "sub": "g-123", "email": "Kavya@Example.com", "email_verified": true, "name": "Kavya", "exp": now() + 600, "iat": now(), "nonce": "n0nce" });

    let id = g.verify(&OAuthLoginRequest { id_token: Some(id_token("k1", good.clone())), nonce: Some("n0nce".into()), ..Default::default() }).await.unwrap();
    assert_eq!((id.subject.as_str(), id.email_verified, id.name.as_deref()), ("g-123", true, Some("Kavya")));
    assert_eq!(g.info().client_id.as_deref(), Some("web-client"));

    let reject = |claims: serde_json::Value, kid: &str, nonce: Option<&str>| {
        let g = g.clone();
        let req = OAuthLoginRequest { id_token: Some(id_token(kid, claims)), nonce: nonce.map(str::to_owned), ..Default::default() };
        async move { g.verify(&req).await.unwrap_err().code }
    };
    let mut wrong_aud = good.clone();
    wrong_aud["aud"] = json!("someone-else");
    assert_eq!(reject(wrong_aud, "k1", None).await, "OAUTH_REJECTED");
    let mut expired = good.clone();
    expired["exp"] = json!(now() - 3600);
    assert_eq!(reject(expired, "k1", None).await, "OAUTH_REJECTED");
    let mut wrong_iss = good.clone();
    wrong_iss["iss"] = json!("https://evil.example");
    assert_eq!(reject(wrong_iss, "k1", None).await, "OAUTH_REJECTED");
    assert_eq!(reject(good.clone(), "k1", Some("other-nonce")).await, "OAUTH_REJECTED");
    assert_eq!(reject(good.clone(), "unknown-kid", None).await, "OAUTH_REJECTED");
    // Missing token is a validation error, not a crash.
    assert_eq!(g.verify(&OAuthLoginRequest::default()).await.unwrap_err().code, "MISSING_TOKEN");
}

#[tokio::test]
async fn apple_accepts_string_email_verified_and_hashed_nonce() {
    let jwks = json!({ "keys": [{ "kid": "a1", "kty": "RSA", "n": N.trim(), "e": "AQAB" }] }).to_string();
    let base = serve(HashMap::from([("/keys", jwks)])).await;
    let a = one(cfg(AuthProvider::Apple, OAuthMode::Live, &["com.feast.web"], None, Some(format!("{base}/keys"))));
    use sha2::Digest;
    let hashed = hex::encode(sha2::Sha256::digest(b"raw-nonce"));
    let claims = json!({ "iss": "https://appleid.apple.com", "aud": "com.feast.web", "sub": "001234.abc", "email": "x@privaterelay.appleid.com", "email_verified": "true", "exp": now() + 600, "iat": now(), "nonce": hashed });
    let id = a
        .verify(&OAuthLoginRequest {
            id_token: Some(id_token("a1", claims)),
            nonce: Some("raw-nonce".into()),
            name: Some("Arjun".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!((id.subject.as_str(), id.email_verified), ("001234.abc", true));
}

#[tokio::test]
async fn facebook_token_must_belong_to_our_app() {
    let graph = serve(HashMap::from([
        ("/debug_token", json!({ "data": { "app_id": "fb-app", "is_valid": true, "user_id": "fb-42" } }).to_string()),
        ("/me", json!({ "id": "fb-42", "name": "Meera", "email": "meera@example.com" }).to_string()),
    ]))
    .await;
    let fb = one(cfg(AuthProvider::Facebook, OAuthMode::Live, &["fb-app"], Some("s3cret"), Some(graph.clone())));
    let id = fb.verify(&OAuthLoginRequest { access_token: Some("EAAB...".into()), ..Default::default() }).await.unwrap();
    assert_eq!((id.subject.as_str(), id.email.as_deref(), id.email_verified), ("fb-42", Some("meera@example.com"), true));

    // Same token, but our app id differs → token was minted for another app.
    let other = one(cfg(AuthProvider::Facebook, OAuthMode::Live, &["our-other-app"], Some("s3cret"), Some(graph)));
    assert_eq!(other.verify(&OAuthLoginRequest { access_token: Some("EAAB...".into()), ..Default::default() }).await.unwrap_err().code, "OAUTH_REJECTED");
}

#[tokio::test]
async fn demo_mode_and_config_validation() {
    use base64::Engine;
    let d = one(cfg(AuthProvider::Google, OAuthMode::Demo, &[], None, None));
    assert!(d.info().demo);
    let tok = format!("demo:{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"email":"Kavya@feast.demo","name":"Kavya"}"#));
    let id = d.verify(&OAuthLoginRequest { id_token: Some(tok), ..Default::default() }).await.unwrap();
    assert_eq!(id.email.as_deref(), Some("kavya@feast.demo"));

    assert!(build(&[cfg(AuthProvider::Google, OAuthMode::Live, &[], None, None)]).is_err(), "live needs client ids");
    assert!(build(&[cfg(AuthProvider::Facebook, OAuthMode::Live, &["app"], None, None)]).is_err(), "facebook needs the app secret");
    assert!(build(&[cfg(AuthProvider::Apple, OAuthMode::Off, &[], None, None)]).unwrap().is_empty());
}
