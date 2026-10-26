//! Stripe (Australia, UK, Canada, NZ…): PaymentIntents + Payment Element.
//!
//! 1. `create_order` → `POST /v1/payment_intents` (automatic payment methods,
//!    so cards, Apple Pay and Google Pay work); the client confirms it with the
//!    `clientSecret` and the publishable key.
//! 2. `charge` retrieves the PaymentIntent server-side and trusts only its
//!    status, amount and currency — never the client.

use std::collections::BTreeMap;

use async_trait::async_trait;
use feast_application::ports::{ChargeOutcome, CheckoutSession, PaymentGateway, RefundOutcome, WebhookEvent};
use feast_application::{AppError, AppResult};
use feast_domain::model::{PaymentIntent, PaymentMethod, PaymentProviderInfo};
use feast_domain::refunds::{Refund, RefundStatus};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use super::{ct_eq, http_client, provider_err};

#[derive(Debug, Clone, Deserialize)]
pub struct StripeConfig {
    pub secret_key: String,
    pub publishable_key: String,
    #[serde(default = "default_base")]
    pub api_base: String,
    /// Signing secret of the webhook endpoint (`whsec_…`). Empty = webhooks rejected.
    #[serde(default)]
    pub webhook_secret: String,
}

/// Reject webhook signatures older/newer than this (replay protection).
const WEBHOOK_TOLERANCE_SECS: i64 = 300;

fn default_base() -> String {
    "https://api.stripe.com".into()
}

pub struct StripeGateway {
    cfg: StripeConfig,
    http: reqwest::Client,
}

impl StripeGateway {
    pub fn new(cfg: StripeConfig) -> AppResult<Self> {
        if cfg.secret_key.is_empty() || cfg.publishable_key.is_empty() {
            return Err(AppError::internal("payments.providers.stripe: secret_key and publishable_key are required"));
        }
        Ok(Self { http: http_client()?, cfg })
    }
}

#[derive(Deserialize)]
struct Intent {
    id: String,
    status: String,
    amount: i64,
    currency: String,
    client_secret: Option<String>,
    latest_charge: Option<serde_json::Value>,
    last_payment_error: Option<serde_json::Value>,
}

/// Card, or the wallet a card came from (Apple Pay / Google Pay).
fn method_of(charge: Option<&serde_json::Value>) -> Option<PaymentMethod> {
    let details = charge?.get("payment_method_details")?;
    match details.get("type")?.as_str()? {
        "card" => match details.pointer("/card/wallet/type").and_then(|v| v.as_str()) {
            Some("apple_pay") => Some(PaymentMethod::ApplePay),
            Some("google_pay") => Some(PaymentMethod::GooglePay),
            _ => Some(PaymentMethod::Card),
        },
        "upi" => Some(PaymentMethod::Upi),
        _ => None,
    }
}

/// Stripe refund status: pending | requires_action | succeeded | failed | canceled.
fn refund_status(s: &str) -> RefundStatus {
    match s {
        "succeeded" => RefundStatus::Succeeded,
        "failed" | "canceled" => RefundStatus::Failed,
        _ => RefundStatus::Pending,
    }
}

#[derive(Deserialize)]
struct StripeRefund {
    id: String,
    status: String,
    #[serde(default)]
    failure_reason: Option<String>,
}

#[derive(Deserialize)]
struct Event {
    #[serde(rename = "type")]
    kind: String,
    data: EventData,
}

#[derive(Deserialize)]
struct EventData {
    object: serde_json::Value,
}

impl StripeGateway {
    /// Verify `Stripe-Signature: t=<unix>,v1=<hex>[,v1=…]` at `now` (unix seconds).
    fn verify_signature(&self, header: &str, body: &[u8], now: i64) -> AppResult<()> {
        let bad = || AppError::new(feast_application::ErrorKind::Unauthenticated, "BAD_SIGNATURE", "Webhook signature doesn't match");
        let mut t = None;
        let mut sigs = vec![];
        for part in header.split(',') {
            match part.trim().split_once('=') {
                Some(("t", v)) => t = v.parse::<i64>().ok(),
                Some(("v1", v)) => sigs.push(v.to_owned()),
                _ => {}
            }
        }
        let t = t.ok_or_else(bad)?;
        if (now - t).abs() > WEBHOOK_TOLERANCE_SECS {
            return Err(bad());
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(self.cfg.webhook_secret.trim().as_bytes()).map_err(|_| bad())?;
        mac.update(t.to_string().as_bytes());
        mac.update(b".");
        mac.update(body);
        let expected = hex::encode(mac.finalize().into_bytes());
        if sigs.iter().any(|s| ct_eq(s.as_bytes(), expected.as_bytes())) {
            Ok(())
        } else {
            Err(bad())
        }
    }

    fn webhook_at(&self, headers: &[(String, String)], body: &[u8], now: i64) -> AppResult<WebhookEvent> {
        if super::webhook_secret(&self.cfg.webhook_secret).is_none() {
            return Err(AppError::new(feast_application::ErrorKind::Unavailable, "WEBHOOK_NOT_CONFIGURED", "Stripe webhook secret isn't configured"));
        }
        let header = headers.iter().find(|(k, _)| k == "stripe-signature").map(|(_, v)| v.as_str()).unwrap_or_default();
        self.verify_signature(header, body, now)?;
        let e: Event = serde_json::from_slice(body).map_err(|err| AppError::validation("BAD_WEBHOOK", format!("stripe webhook: {err}")))?;
        let o = &e.data.object;
        let field = |k: &str| o.get(k).and_then(|v| v.as_str()).map(str::to_owned);
        if matches!(e.kind.as_str(), "refund.created" | "refund.updated" | "refund.failed" | "charge.refund.updated") {
            let Some(id) = field("id") else { return Ok(WebhookEvent::Ignored("refund event without id".into())) };
            let status = if e.kind == "refund.failed" { RefundStatus::Failed } else { refund_status(&field("status").unwrap_or_default()) };
            return Ok(WebhookEvent::Refund {
                provider_refund_id: id,
                status,
                failure: (status == RefundStatus::Failed).then(|| field("failure_reason").unwrap_or_else(|| "Stripe couldn't process the refund".into())),
            });
        }
        let outcome = match e.kind.as_str() {
            "payment_intent.succeeded" => {
                let method = o.pointer("/payment_method_types/0").and_then(|v| v.as_str()).and_then(|t| match t {
                    "card" => Some(PaymentMethod::Card),
                    "upi" => Some(PaymentMethod::Upi),
                    _ => None,
                });
                ChargeOutcome::Succeeded { method, provider_payment_id: field("id") }
            }
            "payment_intent.payment_failed" => {
                ChargeOutcome::Failed(o.pointer("/last_payment_error/message").and_then(|v| v.as_str()).unwrap_or("The payment failed").to_owned())
            }
            other => return Ok(WebhookEvent::Ignored(other.to_owned())),
        };
        let Some(id) = field("id") else { return Ok(WebhookEvent::Ignored("event without payment intent id".into())) };
        Ok(WebhookEvent::Payment { provider_ref: id, outcome, amount: o.get("amount").and_then(|v| v.as_i64()), currency: field("currency") })
    }
}

#[async_trait]
impl PaymentGateway for StripeGateway {
    /// Handles `payment_intent.succeeded` and `payment_intent.payment_failed`.
    fn parse_webhook(&self, headers: &[(String, String)], body: &[u8]) -> AppResult<WebhookEvent> {
        self.webhook_at(headers, body, chrono::Utc::now().timestamp())
    }

    fn info(&self) -> PaymentProviderInfo {
        PaymentProviderInfo {
            id: "stripe".into(),
            name: "Stripe".into(),
            configured: true,
            methods: vec![PaymentMethod::Card, PaymentMethod::ApplePay, PaymentMethod::GooglePay],
        }
    }

    fn public_key(&self) -> Option<String> {
        Some(self.cfg.publishable_key.clone())
    }

    async fn create_order(&self, intent: &PaymentIntent) -> AppResult<CheckoutSession> {
        let form = [
            ("amount", intent.amount.to_string()),
            ("currency", intent.currency.to_lowercase()),
            ("automatic_payment_methods[enabled]", "true".into()),
            ("metadata[intent_id]", intent.id.clone()),
            ("metadata[purpose]", format!("{:?}", intent.purpose)),
        ];
        let res = self
            .http
            .post(format!("{}/v1/payment_intents", self.cfg.api_base))
            .bearer_auth(&self.cfg.secret_key)
            .header("Idempotency-Key", &intent.id)
            .form(&form)
            .send()
            .await
            .map_err(|e| provider_err("stripe", e))?;
        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(provider_err("stripe", format!("create payment intent: {text}")));
        }
        let pi: Intent = res.json().await.map_err(|e| provider_err("stripe", e))?;
        let secret = pi.client_secret.ok_or_else(|| provider_err("stripe", "no client_secret"))?;
        Ok(CheckoutSession {
            provider_ref: Some(pi.id),
            checkout: BTreeMap::from([
                ("mode".into(), "stripe".into()),
                ("publishableKey".into(), self.cfg.publishable_key.clone()),
                ("clientSecret".into(), secret),
            ]),
        })
    }

    /// `POST /v1/refunds` against the PaymentIntent, with our refund id as
    /// the `Idempotency-Key`, so a retry returns the same refund.
    async fn refund(&self, intent: &PaymentIntent, refund: &Refund) -> AppResult<RefundOutcome> {
        let pi = intent
            .provider_ref
            .as_deref()
            .or(intent.provider_payment_id.as_deref())
            .ok_or_else(|| provider_err("stripe", "refund: payment has no PaymentIntent"))?;
        let form = [
            ("payment_intent", pi.to_owned()),
            ("amount", refund.amount.to_string()),
            ("metadata[refund_id]", refund.id.clone()),
            ("metadata[intent_id]", intent.id.clone()),
            ("metadata[reason]", refund.reason.label().to_owned()),
        ];
        let res = self
            .http
            .post(format!("{}/v1/refunds", self.cfg.api_base))
            .bearer_auth(&self.cfg.secret_key)
            .header("Idempotency-Key", &refund.id)
            .form(&form)
            .send()
            .await
            .map_err(|e| provider_err("stripe", e))?;
        let status = res.status();
        if status.is_client_error() && status.as_u16() != 409 && status.as_u16() != 429 {
            let text = res.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.pointer("/error/message").and_then(|d| d.as_str()).map(str::to_owned))
                .unwrap_or_else(|| format!("Stripe refused the refund ({status})"));
            return Ok(RefundOutcome { provider_refund_id: None, status: RefundStatus::Failed, failure: Some(msg) });
        }
        if !status.is_success() {
            return Err(provider_err("stripe", format!("refund: {status}")));
        }
        let r: StripeRefund = res.json().await.map_err(|e| provider_err("stripe", e))?;
        let st = refund_status(&r.status);
        Ok(RefundOutcome { status: st, provider_refund_id: Some(r.id), failure: r.failure_reason.filter(|_| st == RefundStatus::Failed) })
    }

    async fn charge(&self, intent: &PaymentIntent, _method: PaymentMethod, _simulate: bool, _payload: &BTreeMap<String, String>) -> AppResult<ChargeOutcome> {
        let Some(id) = &intent.provider_ref else { return Ok(ChargeOutcome::Failed("Payment wasn't started".into())) };
        let res = self
            .http
            .get(format!("{}/v1/payment_intents/{id}", self.cfg.api_base))
            .query(&[("expand[]", "latest_charge")])
            .bearer_auth(&self.cfg.secret_key)
            .send()
            .await
            .map_err(|e| provider_err("stripe", e))?;
        if !res.status().is_success() {
            return Err(provider_err("stripe", format!("retrieve payment intent: {}", res.status())));
        }
        let pi: Intent = res.json().await.map_err(|e| provider_err("stripe", e))?;
        if pi.amount != intent.amount || !pi.currency.eq_ignore_ascii_case(&intent.currency) {
            return Ok(ChargeOutcome::Failed("Payment details don't match".into()));
        }
        Ok(match pi.status.as_str() {
            "succeeded" => ChargeOutcome::Succeeded { method: method_of(pi.latest_charge.as_ref()), provider_payment_id: Some(pi.id) },
            "processing" => ChargeOutcome::Failed("Your bank is still processing this payment. Check again in a minute.".into()),
            _ => ChargeOutcome::Failed(
                pi.last_payment_error.as_ref().and_then(|e| e.get("message")).and_then(|m| m.as_str()).unwrap_or("The payment wasn't completed").to_owned(),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, t: i64, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{t}.").as_bytes());
        mac.update(body);
        format!("t={t},v1={}", hex::encode(mac.finalize().into_bytes()))
    }

    #[test]
    fn verifies_webhooks() {
        let g = StripeGateway::new(StripeConfig {
            secret_key: "sk_test".into(),
            publishable_key: "pk_test".into(),
            api_base: default_base(),
            webhook_secret: "whsec_test".into(),
        })
        .unwrap();
        let now = 1_800_000_000;
        let body = br#"{"id":"evt_1","type":"payment_intent.succeeded","data":{"object":{"id":"pi_123","amount":6490,"currency":"aud","payment_method_types":["card"],"status":"succeeded"}}}"#;
        let h = |v: String| vec![("stripe-signature".to_owned(), v)];
        assert_eq!(
            g.webhook_at(&h(sign("whsec_test", now, body)), body, now).unwrap(),
            WebhookEvent::Payment {
                provider_ref: "pi_123".into(),
                outcome: ChargeOutcome::Succeeded { method: Some(PaymentMethod::Card), provider_payment_id: Some("pi_123".into()) },
                amount: Some(6490),
                currency: Some("aud".into()),
            }
        );
        // Wrong secret, tampered body, replayed (too old).
        assert_eq!(g.webhook_at(&h(sign("other", now, body)), body, now).unwrap_err().code, "BAD_SIGNATURE");
        assert_eq!(g.webhook_at(&h(sign("whsec_test", now, b"{}")), body, now).unwrap_err().code, "BAD_SIGNATURE");
        assert_eq!(g.webhook_at(&h(sign("whsec_test", now - 3600, body)), body, now).unwrap_err().code, "BAD_SIGNATURE");
        let failed = br#"{"type":"payment_intent.payment_failed","data":{"object":{"id":"pi_123","amount":6490,"currency":"aud","last_payment_error":{"message":"Your card was declined."}}}}"#;
        assert!(matches!(
            g.webhook_at(&h(sign("whsec_test", now, failed)), failed, now).unwrap(),
            WebhookEvent::Payment { outcome: ChargeOutcome::Failed(m), .. } if m == "Your card was declined."
        ));
    }

    #[test]
    fn refund_webhooks() {
        let g = StripeGateway::new(StripeConfig {
            secret_key: "sk_test".into(),
            publishable_key: "pk_test".into(),
            api_base: default_base(),
            webhook_secret: "whsec_test".into(),
        })
        .unwrap();
        let now = 1_800_000_000;
        let h = |v: String| vec![("stripe-signature".to_owned(), v)];
        let ok = br#"{"type":"refund.updated","data":{"object":{"id":"re_1","status":"succeeded","amount":3000,"payment_intent":"pi_123"}}}"#;
        assert_eq!(
            g.webhook_at(&h(sign("whsec_test", now, ok)), ok, now).unwrap(),
            WebhookEvent::Refund { provider_refund_id: "re_1".into(), status: RefundStatus::Succeeded, failure: None }
        );
        let bad = br#"{"type":"refund.failed","data":{"object":{"id":"re_2","status":"failed","failure_reason":"expired_or_canceled_card"}}}"#;
        assert_eq!(
            g.webhook_at(&h(sign("whsec_test", now, bad)), bad, now).unwrap(),
            WebhookEvent::Refund { provider_refund_id: "re_2".into(), status: RefundStatus::Failed, failure: Some("expired_or_canceled_card".into()) }
        );
    }

    #[test]
    fn wallet_methods() {
        let c = serde_json::json!({"payment_method_details": {"type": "card", "card": {"wallet": {"type": "apple_pay"}}}});
        assert_eq!(method_of(Some(&c)), Some(PaymentMethod::ApplePay));
        let c = serde_json::json!({"payment_method_details": {"type": "card", "card": {}}});
        assert_eq!(method_of(Some(&c)), Some(PaymentMethod::Card));
    }
}
