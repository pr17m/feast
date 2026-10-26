//! Push adapters.
//!
//! - [`FcmPushSender`]: Firebase Cloud Messaging HTTP v1. One service account
//!   (JSON key) sends to Android directly and to iOS through APNs (upload the
//!   APNs key in the Firebase console). Access tokens come from Google's OAuth2
//!   service-account flow (a JWT signed with the key's private key) and are
//!   cached until shortly before they expire.
//! - [`LogPushSender`]: logs each push instead of sending (development).

use std::time::{Duration, Instant};

use async_trait::async_trait;
use feast_application::ports::PushSender;
use feast_application::{AppError, AppResult};
use feast_domain::push::{PushMessage, PushOutcome, PushPlatform, PushToken};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::Mutex;

/// Development: log pushes instead of sending them.
pub struct LogPushSender;

#[async_trait]
impl PushSender for LogPushSender {
    fn name(&self) -> &'static str {
        "log"
    }
    async fn send(&self, to: &PushToken, msg: &PushMessage) -> PushOutcome {
        let tail: String = to.token.chars().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
        tracing::info!(user = %to.user_id, platform = to.platform.as_str(), app = to.app.as_str(), token = %format!("…{tail}"), title = %msg.title, link = ?msg.link, "push (log delivery)");
        PushOutcome::Sent
    }
}

/// The parts of a Google service-account JSON key FCM needs.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceAccount {
    pub project_id: String,
    pub client_email: String,
    pub private_key: String,
    #[serde(default = "default_token_uri")]
    pub token_uri: String,
}

fn default_token_uri() -> String {
    "https://oauth2.googleapis.com/token".into()
}

impl ServiceAccount {
    /// From the JSON text (FEAST__PUSH__FCM__CREDENTIALS_JSON) or a file path.
    pub fn load(json_text: &str, file: &str) -> AppResult<Self> {
        let text = if !json_text.trim().is_empty() {
            json_text.to_owned()
        } else if !file.trim().is_empty() {
            std::fs::read_to_string(file).map_err(|e| AppError::internal(format!("push.fcm.credentials_file {file}: {e}")))?
        } else {
            return Err(AppError::internal("push.provider=fcm needs push.fcm.credentials_json or push.fcm.credentials_file"));
        };
        serde_json::from_str(&text).map_err(|e| AppError::internal(format!("push.fcm credentials are not a service-account JSON key: {e}")))
    }
}

pub struct FcmPushSender {
    account: ServiceAccount,
    key: jsonwebtoken::EncodingKey,
    api_base: String,
    http: reqwest::Client,
    access: Mutex<Option<(String, Instant)>>,
}

const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

impl FcmPushSender {
    /// `api_base` overrides `https://fcm.googleapis.com` (tests).
    pub fn new(account: ServiceAccount, api_base: Option<&str>) -> AppResult<Self> {
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(account.private_key.as_bytes())
            .map_err(|e| AppError::internal(format!("push.fcm: the service account's private_key isn't a valid RSA key: {e}")))?;
        let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|e| AppError::internal(e.to_string()))?;
        Ok(Self { account, key, api_base: api_base.unwrap_or("https://fcm.googleapis.com").trim_end_matches('/').to_owned(), http, access: Mutex::new(None) })
    }

    /// A cached OAuth2 access token, refreshed a minute before it expires.
    async fn access_token(&self) -> Result<String, String> {
        let mut cached = self.access.lock().await;
        if let Some((tok, until)) = cached.as_ref() {
            if Instant::now() < *until {
                return Ok(tok.clone());
            }
        }
        let now = chrono::Utc::now().timestamp();
        let claims = json!({ "iss": self.account.client_email, "scope": SCOPE, "aud": self.account.token_uri, "iat": now, "exp": now + 3600 });
        let assertion = jsonwebtoken::encode(&jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256), &claims, &self.key).map_err(|e| e.to_string())?;
        let res = self
            .http
            .post(&self.account.token_uri)
            .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"), ("assertion", assertion.as_str())])
            .send()
            .await
            .map_err(|e| format!("token request: {e}"))?;
        let status = res.status();
        let body: Value = res.json().await.unwrap_or_default();
        let token = body.get("access_token").and_then(Value::as_str).ok_or_else(|| format!("token endpoint {status}: {body}"))?.to_owned();
        let ttl = body.get("expires_in").and_then(Value::as_u64).unwrap_or(3600).saturating_sub(60);
        *cached = Some((token.clone(), Instant::now() + Duration::from_secs(ttl)));
        Ok(token)
    }

    fn body(to: &PushToken, msg: &PushMessage) -> Value {
        let mut data = json!({ "notificationId": msg.notification_id, "kind": msg.kind, "app": to.app.as_str() });
        if let Some(link) = &msg.link {
            data["link"] = json!(link);
        }
        let mut message = json!({
            "token": to.token,
            "notification": { "title": msg.title, "body": msg.body },
            "data": data,
        });
        match to.platform {
            PushPlatform::Android => {
                message["android"] = json!({ "priority": "high", "notification": { "channel_id": "feast", "click_action": "FCM_PLUGIN_ACTIVITY" } })
            }
            PushPlatform::Ios => message["apns"] = json!({ "payload": { "aps": { "sound": "default" } } }),
        }
        json!({ "message": message })
    }
}

/// Worth retrying: FCM overloaded/unavailable, quota, or our access token expired.
fn is_transient(status: u16) -> bool {
    status == 429 || status == 401 || status == 408 || status >= 500
}

/// FCM's verdict on a failed send: the token is gone (forget it) or not.
fn is_unregistered(status: u16, body: &Value) -> bool {
    let err = &body["error"];
    let code = err.get("status").and_then(Value::as_str).unwrap_or_default();
    let detail_codes: Vec<&str> =
        err.get("details").and_then(Value::as_array).into_iter().flatten().filter_map(|d| d.get("errorCode").and_then(Value::as_str)).collect();
    status == 404
        || code == "NOT_FOUND"
        || detail_codes.contains(&"UNREGISTERED")
        || (status == 400 && detail_codes.contains(&"INVALID_ARGUMENT") && body.to_string().contains("registration token"))
}

#[async_trait]
impl PushSender for FcmPushSender {
    fn name(&self) -> &'static str {
        "fcm"
    }

    async fn send(&self, to: &PushToken, msg: &PushMessage) -> PushOutcome {
        let token = match self.access_token().await {
            Ok(t) => t,
            Err(e) => return PushOutcome::Retry(format!("fcm auth: {e}")),
        };
        let url = format!("{}/v1/projects/{}/messages:send", self.api_base, self.account.project_id);
        let res = match self.http.post(&url).bearer_auth(&token).json(&Self::body(to, msg)).send().await {
            Ok(r) => r,
            Err(e) => return PushOutcome::Retry(format!("fcm: {e}")), // timeout / network
        };
        let status = res.status().as_u16();
        if (200..300).contains(&status) {
            return PushOutcome::Sent;
        }
        let body: Value = res.json().await.unwrap_or_default();
        if status == 401 {
            // Token revoked or expired early: fetch a new one next time.
            *self.access.lock().await = None;
        }
        if is_unregistered(status, &body) {
            PushOutcome::Unregistered
        } else if is_transient(status) {
            PushOutcome::Retry(format!("fcm {status}: {body}"))
        } else {
            PushOutcome::Failed(format!("fcm {status}: {body}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unregistered_detection() {
        let gone = json!({"error": {"code": 404, "status": "NOT_FOUND", "details": [{"errorCode": "UNREGISTERED"}]}});
        assert!(is_unregistered(404, &gone));
        let bad = json!({"error": {"code": 400, "status": "INVALID_ARGUMENT", "message": "The registration token is not a valid FCM registration token", "details": [{"errorCode": "INVALID_ARGUMENT"}]}});
        assert!(is_unregistered(400, &bad));
        let quota = json!({"error": {"code": 429, "status": "RESOURCE_EXHAUSTED", "details": [{"errorCode": "QUOTA_EXCEEDED"}]}});
        assert!(!is_unregistered(429, &quota));
        assert!(is_transient(429) && is_transient(503) && is_transient(500) && !is_transient(400) && !is_transient(403));
    }

    #[test]
    fn message_shape_per_platform() {
        let msg = PushMessage {
            title: "T".into(),
            body: "B".into(),
            link: Some("/bookings/bk_1".into()),
            notification_id: "ntf_1".into(),
            kind: "BOOKING_CREATED".into(),
        };
        let t = |platform| PushToken {
            token: "tok_1234567890abcdef".into(),
            user_id: "u_1".into(),
            platform,
            app: feast_domain::push::PushApp::Diner,
            created_at: String::new(),
            last_seen_at: String::new(),
        };
        let a = FcmPushSender::body(&t(PushPlatform::Android), &msg);
        assert_eq!(a["message"]["data"]["link"], "/bookings/bk_1");
        assert_eq!(a["message"]["android"]["priority"], "high");
        let i = FcmPushSender::body(&t(PushPlatform::Ios), &msg);
        assert_eq!(i["message"]["apns"]["payload"]["aps"]["sound"], "default");
        assert!(i["message"].get("android").is_none());
    }
}
