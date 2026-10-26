//! One-time-password adapters.
//!
//! - [`FixedOtpService`]: every phone accepts one configured code (demo/dev).
//! - [`LogOtpService`]: random 6-digit codes with expiry and attempt limits,
//!   "delivered" by logging them. Challenges (a hash of the code, attempts,
//!   expiry) live in the shared database, so any gateway instance can check a
//!   code another one issued.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use feast_application::ports::{OtpService, SharedStateRepository};
use feast_application::{AppError, AppResult};
use rand::Rng;
use sha2::{Digest, Sha256};

pub struct FixedOtpService {
    code: String,
    show_hint: bool,
}

impl FixedOtpService {
    pub fn new(code: impl Into<String>, show_hint: bool) -> Self {
        Self { code: code.into(), show_hint }
    }
}

#[async_trait]
impl OtpService for FixedOtpService {
    async fn send(&self, _phone: &str) -> AppResult<Option<String>> {
        Ok(self.show_hint.then(|| format!("Demo OTP: {}", self.code)))
    }
    async fn verify(&self, _phone: &str, code: &str) -> AppResult<bool> {
        Ok(code == self.code)
    }
}

pub struct LogOtpService {
    ttl: Duration,
    max_attempts: u8,
    store: Arc<dyn SharedStateRepository>,
}

impl LogOtpService {
    pub fn new(ttl: Duration, max_attempts: u8, store: Arc<dyn SharedStateRepository>) -> Self {
        Self { ttl, max_attempts, store }
    }
}

/// Codes are never stored in clear: sha256(phone|code).
fn code_hash(phone: &str, code: &str) -> String {
    hex::encode(Sha256::digest(format!("{phone}|{code}").as_bytes()))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[async_trait]
impl OtpService for LogOtpService {
    async fn send(&self, phone: &str) -> AppResult<Option<String>> {
        let code = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
        self.store.otp_put(phone, &code_hash(phone, &code), now_ms() + self.ttl.as_millis() as i64).await?;
        tracing::info!(phone, %code, "OTP issued (log delivery — wire an SMS provider for production)");
        Ok(None)
    }

    async fn verify(&self, phone: &str, code: &str) -> AppResult<bool> {
        self.store.otp_check(phone, &code_hash(phone, code), now_ms(), i64::from(self.max_attempts)).await
    }
}

/// Twilio Verify: Twilio generates, sends (SMS) and checks the code, with its
/// own rate limits and fraud protection. Works for India and Australia.
/// Config: account SID, auth token and a Verify Service SID.
pub struct TwilioVerifyOtpService {
    account_sid: String,
    auth_token: String,
    service_sid: String,
    api_base: String,
    http: reqwest::Client,
}

impl TwilioVerifyOtpService {
    pub fn new(account_sid: &str, auth_token: &str, service_sid: &str, api_base: Option<&str>) -> AppResult<Self> {
        if account_sid.is_empty() || auth_token.is_empty() || service_sid.is_empty() {
            return Err(AppError::internal("auth.otp.twilio: account_sid, auth_token and service_sid are required"));
        }
        let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|e| AppError::internal(e.to_string()))?;
        Ok(Self {
            account_sid: account_sid.into(),
            auth_token: auth_token.into(),
            service_sid: service_sid.into(),
            api_base: api_base.unwrap_or("https://verify.twilio.com").trim_end_matches('/').into(),
            http,
        })
    }

    async fn post(&self, path: &str, form: &[(&str, &str)]) -> AppResult<serde_json::Value> {
        let res = self
            .http
            .post(format!("{}/v2/Services/{}/{path}", self.api_base, self.service_sid))
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(form)
            .send()
            .await
            .map_err(|e| unavailable(format!("twilio: {e}")))?;
        let status = res.status();
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        if status.as_u16() == 404 && path == "VerificationCheck" {
            // No pending verification (expired or already used).
            return Ok(serde_json::json!({ "status": "expired" }));
        }
        if status.as_u16() == 429 {
            return Err(AppError::new(feast_application::ErrorKind::Conflict, "TOO_MANY_OTPS", "Too many codes requested. Please wait a few minutes."));
        }
        if !status.is_success() {
            return Err(unavailable(format!("twilio {status}: {body}")));
        }
        Ok(body)
    }
}

fn unavailable(detail: String) -> AppError {
    tracing::error!(%detail, "OTP provider error");
    AppError::new(feast_application::ErrorKind::Unavailable, "OTP_UNAVAILABLE", "We couldn't send the code right now. Please try again.")
}

#[async_trait]
impl OtpService for TwilioVerifyOtpService {
    async fn send(&self, phone: &str) -> AppResult<Option<String>> {
        self.post("Verifications", &[("To", phone), ("Channel", "sms")]).await?;
        Ok(None)
    }
    async fn verify(&self, phone: &str, code: &str) -> AppResult<bool> {
        let v = self.post("VerificationCheck", &[("To", phone), ("Code", code)]).await?;
        Ok(v.get("status").and_then(|s| s.as_str()) == Some("approved"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> Arc<dyn SharedStateRepository> {
        Arc::new(crate::sql::SqlStore::connect("sqlite::memory:", 1).await.unwrap())
    }

    /// Issue a code (send() stores a random one and only logs it), then
    /// replace it with a known one so the test can type it.
    async fn issued(s: &Arc<dyn SharedStateRepository>, otp: &LogOtpService, phone: &str) -> String {
        otp.send(phone).await.unwrap();
        let code = "424242".to_owned();
        s.otp_put(phone, &code_hash(phone, &code), now_ms() + 60_000).await.unwrap();
        code
    }

    #[tokio::test]
    async fn log_otp_single_use_and_attempt_limit() {
        let st = store().await;
        let s = LogOtpService::new(Duration::from_secs(60), 2, st.clone());
        let code = issued(&st, &s, "9876543210").await;
        assert!(s.verify("9876543210", &code).await.unwrap());
        assert!(!s.verify("9876543210", &code).await.unwrap(), "codes are single-use");

        let code = issued(&st, &s, "9876543210").await;
        assert!(!s.verify("9876543210", "000000").await.unwrap());
        assert!(!s.verify("9876543210", "111111").await.unwrap());
        assert!(!s.verify("9876543210", &code).await.unwrap(), "locked after max attempts");
    }

    #[tokio::test]
    async fn a_code_issued_by_one_instance_verifies_on_another() {
        let st = store().await;
        let a = LogOtpService::new(Duration::from_secs(60), 5, st.clone());
        let b = LogOtpService::new(Duration::from_secs(60), 5, st.clone());
        let code = issued(&st, &a, "+61400000001").await;
        assert!(b.verify("+61400000001", &code).await.unwrap(), "instance B checks what instance A issued");
        assert!(!a.verify("+61400000001", &code).await.unwrap(), "and it's used up for everyone");
    }

    #[tokio::test]
    async fn expired_codes_fail() {
        let st = store().await;
        let s = LogOtpService::new(Duration::from_secs(60), 5, st.clone());
        st.otp_put("+1", &code_hash("+1", "123456"), now_ms() - 1).await.unwrap();
        assert!(!s.verify("+1", "123456").await.unwrap());
    }
}
