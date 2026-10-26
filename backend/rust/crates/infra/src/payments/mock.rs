//! Test-mode payments: no money moves. The web client shows a simulator
//! (`checkout.mode = "test"`). Refused in production by config validation.

use std::collections::BTreeMap;

use async_trait::async_trait;
use feast_application::ports::{ChargeOutcome, CheckoutSession, PaymentGateway};
use feast_application::AppResult;
use feast_domain::model::{PaymentIntent, PaymentMethod, PaymentProviderInfo};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MockConfig {
    /// Let clients ask for a declined payment (to test failure flows).
    #[serde(default = "yes")]
    pub allow_simulated_failures: bool,
}

fn yes() -> bool {
    true
}

impl Default for MockConfig {
    fn default() -> Self {
        Self { allow_simulated_failures: true }
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
