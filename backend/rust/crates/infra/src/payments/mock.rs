//! Test-mode payments: no money moves. The web client shows a simulator
//! (`checkout.mode = "test"`). Refused in production by config validation.
//!
//! Refunds succeed at once (or stay PENDING with `async_refunds`, finished by a
//! webhook). Test webhooks (`POST /v1/webhooks/payments/mock`) are JSON signed
//! with `X-Mock-Signature` = hex HMAC-SHA256 of the body with `webhook_secret`:
//! `{"type":"payment.succeeded"|"payment.failed","providerRef","amount","currency"}`
//! or `{"type":"refund.succeeded"|"refund.failed","refundId"}`.

use std::collections::BTreeMap;

use async_trait::async_trait;
use feast_application::ports::{ChargeOutcome, CheckoutSession, PaymentGateway, RefundOutcome, WebhookEvent};
use feast_application::{AppError, AppResult, ErrorKind};
use feast_domain::model::{PaymentIntent, PaymentMethod, PaymentProviderInfo};
use feast_domain::refunds::{Refund, RefundStatus};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

#[derive(Debug, Clone, Deserialize)]
pub struct MockConfig {
    /// Let clients ask for a declined payment (to test failure flows).
    #[serde(default = "yes")]
    pub allow_simulated_failures: bool,
    /// Leave refunds PENDING until a test webhook settles them.
    #[serde(default)]
    pub async_refunds: bool,
    /// Signs test webhooks (development only; the mock is refused in production).
    #[serde(default = "default_secret")]
    pub webhook_secret: String,
}

fn default_secret() -> String {
    "mock_webhook_test_secret".into()
}

fn yes() -> bool {
    true
}

impl Default for MockConfig {
    fn default() -> Self {
        Self { allow_simulated_failures: true, async_refunds: false, webhook_secret: default_secret() }
    }
}

pub struct MockPaymentGateway {
    cfg: MockConfig,
}

impl MockPaymentGateway {
    pub fn new(cfg: MockConfig) -> Self {
        Self { cfg }
    }
}

#[async_trait]
impl PaymentGateway for MockPaymentGateway {
    fn info(&self) -> PaymentProviderInfo {
        PaymentProviderInfo { id: "mock".into(), name: "Test payments".into(), configured: true, methods: PaymentMethod::ALL.to_vec() }
    }

    async fn create_order(&self, intent: &PaymentIntent) -> AppResult<CheckoutSession> {
        Ok(CheckoutSession { provider_ref: Some(format!("order_test_{}", intent.id)), checkout: BTreeMap::from([("mode".into(), "test".into())]) })
    }

    async fn refund(&self, intent: &PaymentIntent, refund: &Refund) -> AppResult<RefundOutcome> {
        tracing::info!(intent = %intent.id, refund = %refund.id, amount = refund.amount, "test payments: refund");
        Ok(RefundOutcome {
            provider_refund_id: Some(format!("rfnd_test_{}", refund.id)),
            status: if self.cfg.async_refunds { RefundStatus::Pending } else { RefundStatus::Succeeded },
            failure: None,
        })
    }

    fn parse_webhook(&self, headers: &[(String, String)], body: &[u8]) -> AppResult<WebhookEvent> {
        let Some(secret) = super::webhook_secret(&self.cfg.webhook_secret) else {
            return Err(AppError::new(ErrorKind::Unavailable, "WEBHOOK_NOT_CONFIGURED", "Mock webhook secret isn't configured"));
        };
        let sig = headers.iter().find(|(k, _)| k == "x-mock-signature").map(|(_, v)| v.as_str()).unwrap_or_default();
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(AppError::internal)?;
        mac.update(body);
        if !super::ct_eq(hex::encode(mac.finalize().into_bytes()).as_bytes(), sig.as_bytes()) {
            return Err(AppError::new(ErrorKind::Unauthenticated, "BAD_SIGNATURE", "Webhook signature doesn't match"));
        }
        let v: serde_json::Value = serde_json::from_slice(body).map_err(|e| AppError::validation("BAD_WEBHOOK", format!("mock webhook: {e}")))?;
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_owned);
        Ok(match s("type").as_deref() {
            Some(kind @ ("payment.succeeded" | "payment.failed")) => {
                let Some(provider_ref) = s("providerRef") else { return Ok(WebhookEvent::Ignored("no providerRef".into())) };
                let outcome = if kind == "payment.succeeded" {
                    ChargeOutcome::Succeeded { method: Some(PaymentMethod::Card), provider_payment_id: Some(format!("pay_test_{provider_ref}")) }
                } else {
                    ChargeOutcome::Failed(s("reason").unwrap_or_else(|| "Payment declined (test webhook)".into()))
                };
                WebhookEvent::Payment { provider_ref, outcome, amount: v.get("amount").and_then(|a| a.as_i64()), currency: s("currency") }
            }
            Some(kind @ ("refund.succeeded" | "refund.failed")) => {
                let Some(id) = s("refundId") else { return Ok(WebhookEvent::Ignored("no refundId".into())) };
                let ok = kind == "refund.succeeded";
                WebhookEvent::Refund {
                    provider_refund_id: id,
                    status: if ok { RefundStatus::Succeeded } else { RefundStatus::Failed },
                    failure: (!ok).then(|| "Refund declined (test webhook)".into()),
                }
            }
            other => WebhookEvent::Ignored(other.unwrap_or("unknown").to_owned()),
        })
    }

    async fn charge(
        &self,
        intent: &PaymentIntent,
        method: PaymentMethod,
        simulate_failure: bool,
        _payload: &BTreeMap<String, String>,
    ) -> AppResult<ChargeOutcome> {
        if simulate_failure && self.cfg.allow_simulated_failures {
            tracing::info!(intent = %intent.id, ?method, "test payments: simulated decline");
            return Ok(ChargeOutcome::Failed("Payment declined by bank (simulated).".into()));
        }
        Ok(ChargeOutcome::Succeeded { method: Some(method), provider_payment_id: Some(format!("pay_test_{}", intent.id)) })
    }
}
