//! OpenID Connect ID-token verification (Google, Apple).

use async_trait::async_trait;
use feast_application::ports::{ExternalIdentity, IdentityProvider};
use feast_application::{AppError, AppResult};
use feast_domain::model::{AuthProvider, AuthProviderInfo, OAuthLoginRequest};
use jsonwebtoken::{decode, decode_header, Algorithm, Validation};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{rejected, JwksCache};

#[derive(Debug, Clone)]
pub struct OidcSettings {
    pub provider: AuthProvider,
    /// Accepted `aud` values (all client ids: web, Android, iOS…).
    pub audiences: Vec<String>,
    pub issuers: Vec<String>,
    pub redirect_uri: Option<String>,
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    email: Option<String>,
    /// Google sends a bool; Apple has sent the string "true".
    email_verified: Option<serde_json::Value>,
    name: Option<String>,
    nonce: Option<String>,
}

pub struct OidcIdentityProvider {
    settings: OidcSettings,
    jwks: JwksCache,
}

impl OidcIdentityProvider {
    pub fn new(settings: OidcSettings, jwks: JwksCache) -> Self {
        Self { settings, jwks }
    }
}

#[async_trait]
impl IdentityProvider for OidcIdentityProvider {
    fn info(&self) -> AuthProviderInfo {
        AuthProviderInfo {
            provider: self.settings.provider,
            client_id: self.settings.audiences.first().cloned(),
            redirect_uri: self.settings.redirect_uri.clone(),
            demo: false,
        }
    }

    async fn verify(&self, req: &OAuthLoginRequest) -> AppResult<ExternalIdentity> {
        let p = self.settings.provider;
        let token = req
            .id_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| AppError::validation("MISSING_TOKEN", format!("{} sign-in needs an idToken", p.label())))?;
        let header = decode_header(token).map_err(|e| rejected(p, e))?;
        if header.alg != Algorithm::RS256 {
            return Err(rejected(p, format!("unexpected alg {:?}", header.alg)));
        }
        let kid = header.kid.ok_or_else(|| rejected(p, "no kid"))?;
        let key = self.jwks.key(&kid).await.map_err(|e| if e.kind == feast_application::ErrorKind::Unavailable { e } else { rejected(p, e) })?;

        let mut v = Validation::new(Algorithm::RS256);
        v.set_audience(&self.settings.audiences);
        v.set_issuer(&self.settings.issuers);
        v.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        v.leeway = 60;
        let claims = decode::<Claims>(token, &key, &v).map_err(|e| rejected(p, e))?.claims;

        if let Some(expected) = req.nonce.as_deref() {
            // Clients either send the raw nonce or its SHA-256 (Apple's recommendation).
            let hashed = hex::encode(Sha256::digest(expected.as_bytes()));
            if claims.nonce.as_deref() != Some(expected) && claims.nonce.as_deref() != Some(hashed.as_str()) {
                return Err(rejected(p, "nonce mismatch"));
            }
        }

        let verified = matches!(claims.email_verified, Some(serde_json::Value::Bool(true)))
            || matches!(&claims.email_verified, Some(serde_json::Value::String(s)) if s == "true");
        Ok(ExternalIdentity { provider: p, subject: claims.sub, email: claims.email, email_verified: verified, name: claims.name })
    }
}
