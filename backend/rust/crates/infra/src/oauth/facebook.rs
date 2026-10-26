//! Facebook Login: verify a user access token server-side.
//!
//! 1. `GET /debug_token` with the app token → the token is valid and was issued
//!    to *our* app (prevents tokens from other apps being replayed here).
//! 2. `GET /me?fields=id,name,email` with `appsecret_proof` → the profile.

use async_trait::async_trait;
use feast_application::ports::{ExternalIdentity, IdentityProvider};
use feast_application::{AppError, AppResult};
use feast_domain::model::{AuthProvider, AuthProviderInfo, OAuthLoginRequest};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;

use super::rejected;

const P: AuthProvider = AuthProvider::Facebook;

#[derive(Deserialize)]
struct DebugToken {
    data: DebugData,
}

#[derive(Deserialize)]
struct DebugData {
    app_id: Option<String>,
    is_valid: Option<bool>,
    user_id: Option<String>,
}

#[derive(Deserialize)]
struct Me {
    id: String,
    name: Option<String>,
    email: Option<String>,
}

pub struct FacebookIdentityProvider {
    app_id: String,
    app_secret: String,
    graph: String,
    trust_email: bool,
    http: reqwest::Client,
}

impl FacebookIdentityProvider {
    pub fn new(app_id: String, app_secret: String, graph: String, trust_email: bool, http: reqwest::Client) -> Self {
        Self { app_id, app_secret, graph: graph.trim_end_matches('/').to_owned(), trust_email, http }
    }

    fn proof(&self, token: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.app_secret.as_bytes()).expect("hmac accepts any key length");
        mac.update(token.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str, query: &[(&str, &str)]) -> AppResult<T> {
        let res = self.http.get(format!("{}{path}", self.graph)).query(query).send().await.map_err(|e| {
            tracing::warn!(error = %e, "facebook graph unreachable");
            AppError::unavailable("Facebook is unreachable. Please try again.")
        })?;
        if !res.status().is_success() {
            return Err(rejected(P, format!("graph {path} returned {}", res.status())));
        }
        res.json().await.map_err(|e| rejected(P, e))
    }
}

#[async_trait]
impl IdentityProvider for FacebookIdentityProvider {
    fn info(&self) -> AuthProviderInfo {
        AuthProviderInfo { provider: P, client_id: Some(self.app_id.clone()), redirect_uri: None, demo: false }
    }

    async fn verify(&self, req: &OAuthLoginRequest) -> AppResult<ExternalIdentity> {
        let token = req
            .access_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| AppError::validation("MISSING_TOKEN", "Facebook sign-in needs an accessToken"))?;
        let app_token = format!("{}|{}", self.app_id, self.app_secret);
        let dbg: DebugToken = self.get("/debug_token", &[("input_token", token), ("access_token", &app_token)]).await?;
        if dbg.data.is_valid != Some(true) {
            return Err(rejected(P, "token invalid or expired"));
        }
        if dbg.data.app_id.as_deref() != Some(self.app_id.as_str()) {
            return Err(rejected(P, "token was issued to a different app"));
        }
        let proof = self.proof(token);
        let me: Me = self.get("/me", &[("fields", "id,name,email"), ("access_token", token), ("appsecret_proof", &proof)]).await?;
        if dbg.data.user_id.as_deref().is_some_and(|u| u != me.id) {
            return Err(rejected(P, "user mismatch"));
        }
        Ok(ExternalIdentity { provider: P, subject: me.id, email_verified: self.trust_email && me.email.is_some(), email: me.email, name: me.name })
    }
}
