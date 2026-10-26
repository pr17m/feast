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
use feast_application::ports::{ChargeOutcome, CheckoutSession, PaymentGateway, RefundOutcome, WebhookEvent};
use feast_application::{AppError, AppResult};
use feast_domain::model::{PaymentIntent, PaymentMethod, PaymentProviderInfo};
use feast_domain::refunds::{Refund, RefundStatus};
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

    /// The captured payment of an order (for intents settled before payment ids were stored).
    async fn captured_payment_of(&self, intent: &PaymentIntent) -> AppResult<String> {
        let order = intent.provider_ref.as_deref().ok_or_else(|| provider_err("razorpay", "refund: payment has no order"))?;
        let res = self
            .http
            .get(format!("{}/v1/orders/{order}/payments", self.cfg.api_base))
            .basic_auth(&self.cfg.key_id, Some(&self.cfg.key_secret))
            .send()
            .await
            .map_err(|e| provider_err("razorpay", e))?;
        if !res.status().is_success() {
            return Err(provider_err("razorpay", format!("order payments: {}", res.status())));
        }
        let list: PaymentList = res.json().await.map_err(|e| provider_err("razorpay", e))?;
        list.items
            .into_iter()
            .find(|p| p.status == "captured")
            .and_then(|p| p.id)
            .ok_or_else(|| provider_err("razorpay", "refund: no captured payment on the order"))
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
    refund: Option<Wrapped<RefundEntity>>,
}

/// A Razorpay refund (`rfnd_…`): status pending | processed | failed.
#[derive(Deserialize)]
struct RefundEntity {
    id: String,
    status: String,
    #[serde(default)]
    receipt: Option<String>,
}

#[derive(Deserialize)]
struct RefundList {
    items: Vec<RefundEntity>,
}

#[derive(Deserialize)]
struct PaymentList {
    items: Vec<Payment>,
}

fn refund_status(s: &str) -> RefundStatus {
    match s {
        "processed" => RefundStatus::Succeeded,
        "failed" => RefundStatus::Failed,
        _ => RefundStatus::Pending,
    }
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
        if w.event.starts_with("refund.") {
            // refund.created / refund.processed / refund.failed (payload also carries the payment).
            return Ok(match w.payload.refund.map(|r| r.entity) {
                Some(r) => WebhookEvent::Refund {
                    status: refund_status(&r.status),
                    failure: (r.status == "failed").then(|| "Razorpay couldn't process the refund".to_owned()),
                    provider_refund_id: r.id,
                },
                None => WebhookEvent::Ignored(w.event),
            });
        }
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

    /// `POST /v1/payments/{id}/refund` with `receipt` = our refund id. Before
    /// sending, look for a refund with that receipt (a retry after a lost
    /// answer), so the same refund is never made twice.
    async fn refund(&self, intent: &PaymentIntent, refund: &Refund) -> AppResult<RefundOutcome> {
        let payment_id = match &intent.provider_payment_id {
            Some(id) => id.clone(),
            None => self.captured_payment_of(intent).await?,
        };
        let auth = (&self.cfg.key_id, Some(&self.cfg.key_secret));
        let res = self
            .http
            .get(format!("{}/v1/payments/{payment_id}/refunds", self.cfg.api_base))
            .basic_auth(auth.0, auth.1)
            .send()
            .await
            .map_err(|e| provider_err("razorpay", e))?;
        if res.status().is_success() {
            if let Ok(list) = res.json::<RefundList>().await {
                if let Some(r) = list.items.into_iter().find(|r| r.receipt.as_deref() == Some(refund.id.as_str())) {
                    return Ok(RefundOutcome { status: refund_status(&r.status), provider_refund_id: Some(r.id), failure: None });
                }
            }
        }
        let body = serde_json::json!({
            "amount": refund.amount,
            "speed": "normal",
            "receipt": refund.id,
            "notes": { "feast_refund_id": refund.id, "intent_id": intent.id, "reason": refund.reason.label() },
        });
        let res = self
            .http
            .post(format!("{}/v1/payments/{payment_id}/refund", self.cfg.api_base))
            .basic_auth(auth.0, auth.1)
            .json(&body)
            .send()
            .await
            .map_err(|e| provider_err("razorpay", e))?;
        let status = res.status();
        if status.is_client_error() {
            // Refused (e.g. amount above what's left): final.
            let text = res.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.pointer("/error/description").and_then(|d| d.as_str()).map(str::to_owned))
                .unwrap_or_else(|| format!("Razorpay refused the refund ({status})"));
            return Ok(RefundOutcome { provider_refund_id: None, status: RefundStatus::Failed, failure: Some(msg) });
        }
        if !status.is_success() {
            return Err(provider_err("razorpay", format!("refund: {status}")));
        }
        let r: RefundEntity = res.json().await.map_err(|e| provider_err("razorpay", e))?;
        Ok(RefundOutcome { status: refund_status(&r.status), provider_refund_id: Some(r.id), failure: None })
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
        let refunded = br#"{"event":"refund.processed","payload":{"refund":{"entity":{"id":"rfnd_1","payment_id":"pay_9","amount":5000,"status":"processed"}},"payment":{"entity":{"id":"pay_9","order_id":"order_1","amount":120000,"currency":"INR","status":"refunded"}}}}"#;
        assert_eq!(
            g.parse_webhook(&h(&hmac_hex("whsec_rzp_test", refunded)), refunded).unwrap(),
            WebhookEvent::Refund { provider_refund_id: "rfnd_1".into(), status: RefundStatus::Succeeded, failure: None }
        );
        let rfail = br#"{"event":"refund.failed","payload":{"refund":{"entity":{"id":"rfnd_2","status":"failed"}}}}"#;
        assert!(matches!(g.parse_webhook(&h(&hmac_hex("whsec_rzp_test", rfail)), rfail).unwrap(), WebhookEvent::Refund { status: RefundStatus::Failed, .. }));

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
