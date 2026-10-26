//! Social login adapters (`IdentityProvider` port).
//!
//! | Provider | Client gets            | Server verifies                                    |
//! |----------|------------------------|----------------------------------------------------|
//! | Google   | OIDC ID token (JWT)    | signature via Google JWKS, `aud`, `iss`, `exp`     |
//! | Apple    | OIDC ID token (JWT)    | signature via Apple JWKS, `aud`, `iss`, `exp`      |
//! | Facebook | user access token      | `debug_token` (app match, validity) + Graph `/me`  |
//!
//! The same endpoint serves web and the future Android/iOS apps — only the
//! accepted client ids (`aud`) differ, so list them all in config.
//! `demo` mode needs no credentials (for local dev and the mock webapp).

mod demo;
mod facebook;
mod jwks;
mod oidc;

use std::sync::Arc;
use std::time::Duration;

pub use demo::DemoIdentityProvider;
pub use facebook::FacebookIdentityProvider;
pub use jwks::JwksCache;
pub use oidc::{OidcIdentityProvider, OidcSettings};

use feast_application::ports::IdentityProvider;
use feast_application::{AppError, AppResult};
use feast_domain::model::AuthProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthMode {
    Off,
    /// Fake provider: accepts demo credentials. Never enable in production.
    Demo,
    Live,
}

#[derive(Debug, Clone)]
pub struct OAuthProviderConfig {
    pub provider: AuthProvider,
    pub mode: OAuthMode,
    /// Accepted OAuth client ids: web + Android + iOS (Google), Services ID + bundle id (Apple), app id (Facebook).
    pub client_ids: Vec<String>,
    /// Facebook app secret (Apple/Google don't need one for ID-token verification).
    pub client_secret: Option<String>,
    /// Apple JS redirect URI (sent to the web client).
    pub redirect_uri: Option<String>,
    /// Treat the provider's email as verified (Facebook gives no verified flag).
    pub trust_email: bool,
    /// Override provider endpoints (tests, proxies). None = the real provider.
    pub endpoint_override: Option<String>,
}

pub const GOOGLE_JWKS: &str = "https://www.googleapis.com/oauth2/v3/certs";
pub const APPLE_JWKS: &str = "https://appleid.apple.com/auth/keys";
pub const FACEBOOK_GRAPH: &str = "https://graph.facebook.com/v19.0";

fn http() -> AppResult<reqwest::Client> {
    reqwest::Client::builder().timeout(Duration::from_secs(8)).user_agent("feast-backend").build().map_err(AppError::internal)
}

/// Build the enabled providers from config.
pub fn build(configs: &[OAuthProviderConfig]) -> AppResult<Vec<Arc<dyn IdentityProvider>>> {
    let mut out: Vec<Arc<dyn IdentityProvider>> = vec![];
    for c in configs {
        let name = c.provider.slug();
        match c.mode {
            OAuthMode::Off => continue,
            OAuthMode::Demo => {
                tracing::warn!(provider = name, "social login in DEMO mode — anyone can sign in as any email. Never use in production.");
                out.push(Arc::new(DemoIdentityProvider::new(c.provider)));
            }
            OAuthMode::Live => {
                if c.client_ids.is_empty() {
                    return Err(AppError::validation("OAUTH_CONFIG", format!("auth.oauth.{name}.client_ids is required in live mode")));
                }
                let provider: Arc<dyn IdentityProvider> = match c.provider {
                    AuthProvider::Google => Arc::new(OidcIdentityProvider::new(
                        OidcSettings {
                            provider: AuthProvider::Google,
                            audiences: c.client_ids.clone(),
                            issuers: vec!["https://accounts.google.com".into(), "accounts.google.com".into()],
                            redirect_uri: None,
                        },
                        JwksCache::new(c.endpoint_override.clone().unwrap_or_else(|| GOOGLE_JWKS.into()), http()?),
                    )),
                    AuthProvider::Apple => Arc::new(OidcIdentityProvider::new(
                        OidcSettings {
                            provider: AuthProvider::Apple,
                            audiences: c.client_ids.clone(),
                            issuers: vec!["https://appleid.apple.com".into()],
                            redirect_uri: c.redirect_uri.clone(),
                        },
                        JwksCache::new(c.endpoint_override.clone().unwrap_or_else(|| APPLE_JWKS.into()), http()?),
                    )),
                    AuthProvider::Facebook => {
                        let secret =
                            c.client_secret.clone().filter(|s| !s.is_empty()).ok_or_else(|| {
                                AppError::validation("OAUTH_CONFIG", "auth.oauth.facebook.client_secret (app secret) is required in live mode")
                            })?;
                        Arc::new(FacebookIdentityProvider::new(
                            c.client_ids[0].clone(),
                            secret,
                            c.endpoint_override.clone().unwrap_or_else(|| FACEBOOK_GRAPH.into()),
                            c.trust_email,
                            http()?,
                        ))
                    }
                };
                tracing::info!(provider = name, "social login enabled");
                out.push(provider);
            }
        }
    }
    Ok(out)
}

pub(crate) fn rejected(provider: AuthProvider, why: impl std::fmt::Display) -> AppError {
    tracing::info!(provider = provider.slug(), reason = %why, "social login rejected");
    AppError::new(
        feast_application::ErrorKind::Unauthenticated,
        "OAUTH_REJECTED",
        format!("We couldn't verify your {} sign-in. Please try again.", provider.label()),
    )
}
