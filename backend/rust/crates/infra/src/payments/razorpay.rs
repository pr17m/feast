//! Razorpay (India): Orders API + Checkout.js.
//!
//! 1. `create_order` → `POST /v1/orders`; the client opens Checkout with the
//!    order id and the public key id.
//! 2. Checkout returns `razorpay_payment_id`, `razorpay_order_id`,
//!    `razorpay_signature`; `charge` verifies the HMAC-SHA256 signature
//!    (`order_id|payment_id` with the key secret), then fetches the payment to
//!    check amount, currency and status, and read the method used.

use std::collections::BTreeMap;

use async_trait::async_trait;
use feast_application::ports::{ChargeOutcome, CheckoutSession, PaymentGateway, WebhookEvent};
use feast_application::{AppError, AppResult};
use feast_domain::model::{PaymentIntent, PaymentMethod, PaymentProviderInfo};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use super::{ct_eq, http_client, provider_err};

#[derive(Debug, Clone, Deserialize)]
pub struct RazorpayConfig {
    pub key_id: String,
    pub key_secret: String,
    #[serde(default = "default_base")]
    pub api_base: String,
    /// Display name in the checkout sheet.
    #[serde(default = "default_name")]
    pub merchant_name: String,
    /// Webhook secret (Dashboard → Webhooks). Empty = webhooks rejected.
    #[serde(default)]
    pub webhook_secret: String,
}

fn default_base() -> String {
    "https://api.razorpay.com".into()
}
fn default_name() -> String {
    "Feast".into()
}

pub struct RazorpayGateway {
    cfg: RazorpayConfig,
    http: reqwest::Client,
}

impl RazorpayGateway {
    pub fn new(cfg: RazorpayConfig) -> AppResult<Self> {
        if cfg.key_id.is_empty() || cfg.key_secret.is_empty() {
            return Err(AppError::internal("payments.providers.razorpay: key_id and key_secret are required"));
        }
        Ok(Self { http: http_client()?, cfg })
    }

    fn signature_ok(&self, order_id: &str, payment_id: &str, signature: &str) -> bool {
        let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(self.cfg.key_secret.as_bytes()) else { return false };
        mac.update(format!("{order_id}|{payment_id}").as_bytes());
        ct_eq(hex::encode(mac.finalize().into_bytes()).as_bytes(), signature.as_bytes())
    }
}

#[derive(Deserialize)]
struct Order {
    id: String,
}

#[derive(Deserialize)]
struct Payment {
    #[serde(default)]
    id: Option<String>,
    status: String,
    amount: i64,
    currency: String,
    order_id: Option<String>,
    method: Option<String>,
    error_description: Option<String>,
}

fn method_of(m: Option<&str>) -> Option<PaymentMethod> {
    match m? {
        "upi" => Some(PaymentMethod::Upi),
        "card" => Some(PaymentMethod::Card),
        "netbanking" => Some(PaymentMethod::Netbanking),
        "wallet" => Some(PaymentMethod::Wallet),
        _ => None,
    }
}

/// Webhook body: `{ event, payload: { payment: { entity }, order?: { entity } } }`.
#[derive(Deserialize)]
struct Webhook {
    event: String,
    payload: WebhookPayload,
}

#[derive(Deserialize)]
struct WebhookPayload {
    payment: Option<Wrapped<Payment>>,
}

#[derive(Deserialize)]
struct Wrapped<T> {
    entity: T,
}

fn hmac_hex(secret: &str, data: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(data);
    hex::encode(mac.finalize().into_bytes())
}

#[async_trait]
impl PaymentGateway for RazorpayGateway {
    /// `X-Razorpay-Signature` = hex HMAC-SHA256 of the raw body with the webhook
    /// secret. Handles `payment.captured`, `order.paid` and `payment.failed`.
    fn parse_webhook(&self, headers: &[(String, String)], body: &[u8]) -> AppResult<WebhookEvent> {
        let Some(secret) = super::webhook_secret(&self.cfg.webhook_secret) else {
            return Err(AppError::new(feast_application::ErrorKind::Unavailable, "WEBHOOK_NOT_CONFIGURED", "Razorpay webhook secret isn't configured"));
        };
        let sig = headers.iter().find(|(k, _)| k == "x-razorpay-signature").map(|(_, v)| v.as_str()).unwrap_or_default();
        if !ct_eq(hmac_hex(secret, body).as_bytes(), sig.as_bytes()) {
            return Err(AppError::new(feast_application::ErrorKind::Unauthenticated, "BAD_SIGNATURE", "Webhook signature doesn't match"));
        }
        let w: Webhook = serde_json::from_slice(body).map_err(|e| AppError::validation("BAD_WEBHOOK", format!("razorpay webhook: {e}")))?;
        let Some(p) = w.payload.payment.map(|p| p.entity) else { return Ok(WebhookEvent::Ignored(w.event)) };
        let Some(order_id) = p.order_id.clone() else { return Ok(WebhookEvent::Ignored("payment without order".into())) };
        let outcome = match w.event.as_str() {
            "payment.captured" | "order.paid" => ChargeOutcome::Succeeded { method: method_of(p.method.as_deref()), provider_payment_id: p.id.clone() },
            "payment.failed" => ChargeOutcome::Failed(p.error_description.clone().unwrap_or_else(|| "Payment failed".into())),
            other => return Ok(WebhookEvent::Ignored(other.to_owned())),
        };
        Ok(WebhookEvent::Payment { provider_ref: order_id, outcome, amount: Some(p.amount), currency: Some(p.currency) })
    }

    fn info(&self) -> PaymentProviderInfo {
        PaymentProviderInfo {
            id: "razorpay".into(),
            name: "Razorpay".into(),
            configured: true,
            methods: vec![PaymentMethod::Upi, PaymentMethod::Card, PaymentMethod::Netbanking, PaymentMethod::Wallet],
        }
    }

    fn public_key(&self) -> Option<String> {
        Some(self.cfg.key_id.clone())
    }

    async fn create_order(&self, intent: &PaymentIntent) -> AppResult<CheckoutSession> {
        let body = serde_json::json!({
            "amount": intent.amount,
            "currency": intent.currency,
            "receipt": intent.id,
            "notes": { "intent_id": intent.id, "purpose": format!("{:?}", intent.purpose) },
        });
        let res = self
            .http
            .post(format!("{}/v1/orders", self.cfg.api_base))
            .basic_auth(&self.cfg.key_id, Some(&self.cfg.key_secret))
            .json(&body)
            .send()
            .await
            .map_err(|e| provider_err("razorpay", e))?;
        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(provider_err("razorpay", format!("create order: {text}")));
        }
        let order: Order = res.json().await.map_err(|e| provider_err("razorpay", e))?;
        Ok(CheckoutSession {
            provider_ref: Some(order.id.clone()),
            checkout: BTreeMap::from([
                ("mode".into(), "razorpay".into()),
                ("keyId".into(), self.cfg.key_id.clone()),
                ("orderId".into(), order.id),
                ("name".into(), self.cfg.merchant_name.clone()),
            ]),
        })
    }

    async fn charge(&self, intent: &PaymentIntent, _method: PaymentMethod, _simulate: bool, payload: &BTreeMap<String, String>) -> AppResult<ChargeOutcome> {
        let (Some(payment_id), Some(order_id), Some(signature)) =
            (payload.get("razorpay_payment_id"), payload.get("razorpay_order_id"), payload.get("razorpay_signature"))
        else {
            return Ok(ChargeOutcome::Failed("The payment wasn't completed".into()));
        };
        if intent.provider_ref.as_deref() != Some(order_id.as_str()) || !self.signature_ok(order_id, payment_id, signature) {
            tracing::warn!(intent = %intent.id, "razorpay: signature mismatch");
            return Ok(ChargeOutcome::Failed("We couldn't verify this payment".into()));
        }
        let res = self
            .http
            .get(format!("{}/v1/payments/{payment_id}", self.cfg.api_base))
            .basic_auth(&self.cfg.key_id, Some(&self.cfg.key_secret))
            .send()
            .await
            .map_err(|e| provider_err("razorpay", e))?;
        if !res.status().is_success() {
            return Err(provider_err("razorpay", format!("fetch payment: {}", res.status())));
        }
        let p: Payment = res.json().await.map_err(|e| provider_err("razorpay", e))?;
        if p.order_id.as_deref() != Some(order_id.as_str()) || p.amount != intent.amount || !p.currency.eq_ignore_ascii_case(&intent.currency) {
            return Ok(ChargeOutcome::Failed("Payment details don't match".into()));
        }
        match p.status.as_str() {
            // "authorized" is captured automatically when the order uses payment_capture (account default).
            "captured" | "authorized" => Ok(ChargeOutcome::Succeeded { method: method_of(p.method.as_deref()), provider_payment_id: Some(payment_id.clone()) }),
            _ => Ok(ChargeOutcome::Failed(p.error_description.unwrap_or_else(|| "Payment failed".into()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway() -> RazorpayGateway {
        RazorpayGateway::new(RazorpayConfig {
            key_id: "rzp_test_x".into(),
            key_secret: "secret".into(),
            api_base: default_base(),
            merchant_name: "Feast".into(),
            webhook_secret: "whsec_rzp_test".into(),
        })
        .unwrap()
    }

    #[test]
    fn verifies_webhooks() {
        let g = gateway();
        let body = br#"{"event":"payment.captured","payload":{"payment":{"entity":{"id":"pay_9","order_id":"order_1","amount":120000,"currency":"INR","status":"captured","method":"upi"}}}}"#;
        let h = |sig: &str| vec![("x-razorpay-signature".to_owned(), sig.to_owned())];
        let ok = g.parse_webhook(&h(&hmac_hex("whsec_rzp_test", body)), body).unwrap();
        assert_eq!(
            ok,
            WebhookEvent::Payment {
                provider_ref: "order_1".into(),
                outcome: ChargeOutcome::Succeeded { method: Some(PaymentMethod::Upi), provider_payment_id: Some("pay_9".into()) },
                amount: Some(120000),
                currency: Some("INR".into()),
            }
        );
        assert_eq!(g.parse_webhook(&h("forged"), body).unwrap_err().code, "BAD_SIGNATURE");
        let failed = br#"{"event":"payment.failed","payload":{"payment":{"entity":{"id":"pay_9","order_id":"order_1","amount":120000,"currency":"INR","status":"failed","error_description":"Bank declined"}}}}"#;
        assert!(
            matches!(g.parse_webhook(&h(&hmac_hex("whsec_rzp_test", failed)), failed).unwrap(), WebhookEvent::Payment { outcome: ChargeOutcome::Failed(r), .. } if r == "Bank declined")
        );
        let other = br#"{"event":"refund.created","payload":{}}"#;
        assert!(matches!(g.parse_webhook(&h(&hmac_hex("whsec_rzp_test", other)), other).unwrap(), WebhookEvent::Ignored(_)));

        // Deploy placeholders ("-" for an unset secret) never verify anything.
        for placeholder in ["", "-", " - ", "short"] {
            let g = RazorpayGateway::new(RazorpayConfig { webhook_secret: placeholder.into(), ..gateway().cfg.clone() }).unwrap();
            assert_eq!(g.parse_webhook(&h(&hmac_hex(placeholder, body)), body).unwrap_err().code, "WEBHOOK_NOT_CONFIGURED");
        }
    }

    #[test]
    fn verifies_checkout_signature() {
        let g = gateway();
        let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
        mac.update(b"order_1|pay_1");
        let sig = hex::encode(mac.finalize().into_bytes());
        assert!(g.signature_ok("order_1", "pay_1", &sig));
        assert!(!g.signature_ok("order_1", "pay_2", &sig));
    }
}
