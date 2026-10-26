//! Demo social login: no real provider. The client sends
//! `demo:<base64url(JSON {"email","name"})>` as the token. For local
//! development and the mock webapp only — anyone can claim any email.

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use feast_application::ports::{ExternalIdentity, IdentityProvider};
use feast_application::{AppError, AppResult};
use feast_domain::model::{AuthProvider, AuthProviderInfo, OAuthLoginRequest};
use serde::Deserialize;

pub struct DemoIdentityProvider {
    provider: AuthProvider,
}

impl DemoIdentityProvider {
    pub fn new(provider: AuthProvider) -> Self {
        Self { provider }
    }
}

#[derive(Deserialize)]
struct DemoClaims {
    email: String,
    name: Option<String>,
}

#[async_trait]
impl IdentityProvider for DemoIdentityProvider {
    fn info(&self) -> AuthProviderInfo {
        AuthProviderInfo { provider: self.provider, client_id: None, redirect_uri: None, demo: true }
    }

    async fn verify(&self, req: &OAuthLoginRequest) -> AppResult<ExternalIdentity> {
        let bad = || AppError::validation("BAD_DEMO_TOKEN", "Demo sign-in token is malformed");
        let token = req.id_token.as_deref().or(req.access_token.as_deref()).ok_or_else(bad)?;
        let body = token.strip_prefix("demo:").ok_or_else(bad)?;
        let json = URL_SAFE_NO_PAD.decode(body.trim_end_matches('=')).map_err(|_| bad())?;
        let c: DemoClaims = serde_json::from_slice(&json).map_err(|_| bad())?;
        let email = c.email.trim().to_lowercase();
        if !email.contains('@') {
            return Err(bad());
        }
        Ok(ExternalIdentity { provider: self.provider, subject: format!("demo|{email}"), email: Some(email), email_verified: true, name: c.name })
    }
}
