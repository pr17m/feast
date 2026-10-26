//! One-time-password adapters.
//!
//! - [`FixedOtpService`]: every phone accepts one configured code (demo/dev).
//! - [`LogOtpService`]: random 6-digit codes with expiry and attempt limits,
//!   "delivered" by logging them. Replace the log line with an SMS provider
//!   (MSG91, Twilio…) to go live; storage moves to Redis when running several
//!   gateway instances.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use feast_application::ports::OtpService;
use feast_application::{AppError, AppResult};
use rand::Rng;

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

struct Pending {
    code: String,
    expires: Instant,
    attempts: u8,
}

pub struct LogOtpService {
    ttl: Duration,
    max_attempts: u8,
    codes: Mutex<HashMap<String, Pending>>,
}

impl LogOtpService {
    pub fn new(ttl: Duration, max_attempts: u8) -> Self {
        Self { ttl, max_attempts, codes: Mutex::new(HashMap::new()) }
    }
}

#[async_trait]
impl OtpService for LogOtpService {
    async fn send(&self, phone: &str) -> AppResult<Option<String>> {
        let code = format!("{:06}", rand::thread_rng().gen_range(0..1_000_000));
        tracing::info!(phone, %code, "OTP issued (log delivery — wire an SMS provider for production)");
        let mut codes = self.codes.lock().map_err(AppError::internal)?;
        codes.retain(|_, p| p.expires > Instant::now());
        codes.insert(phone.to_owned(), Pending { code, expires: Instant::now() + self.ttl, attempts: 0 });
        Ok(None)
    }

    async fn verify(&self, phone: &str, code: &str) -> AppResult<bool> {
        let mut codes = self.codes.lock().map_err(AppError::internal)?;
        let Some(p) = codes.get_mut(phone) else { return Ok(false) };
        if p.expires <= Instant::now() || p.attempts >= self.max_attempts {
            codes.remove(phone);
            return Ok(false);
        }
        p.attempts += 1;
        let ok = p.code == code;
        if ok {
            codes.remove(phone);
        }
        Ok(ok)
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

    #[tokio::test]
    async fn log_otp_single_use_and_attempt_limit() {
        let s = LogOtpService::new(Duration::from_secs(60), 2);
        s.send("9876543210").await.unwrap();
        let code = s.codes.lock().unwrap().get("9876543210").unwrap().code.clone();
        assert!(s.verify("9876543210", &code).await.unwrap());
        assert!(!s.verify("9876543210", &code).await.unwrap(), "codes are single-use");

        s.send("9876543210").await.unwrap();
        let code = s.codes.lock().unwrap().get("9876543210").unwrap().code.clone();
        let wrong = if code == "000000" { "111111" } else { "000000" };
        assert!(!s.verify("9876543210", wrong).await.unwrap());
        assert!(!s.verify("9876543210", wrong).await.unwrap());
        assert!(!s.verify("9876543210", &code).await.unwrap(), "locked after max attempts");
    }
}
