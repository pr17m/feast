//! Cached JSON Web Key Sets (Google/Apple signing keys).
//! Keys are cached for an hour; an unknown `kid` (key rotation) triggers a
//! refresh, at most once every 30 seconds.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use feast_application::{AppError, AppResult};
use jsonwebtoken::DecodingKey;
use serde::Deserialize;
use tokio::sync::RwLock;

const TTL: Duration = Duration::from_secs(3600);
const MIN_REFRESH: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Deserialize)]
struct Jwk {
    kid: Option<String>,
    kty: String,
    n: Option<String>,
    e: Option<String>,
}

#[derive(Default)]
struct State {
    keys: HashMap<String, DecodingKey>,
    fetched: Option<Instant>,
}

pub struct JwksCache {
    url: String,
    http: reqwest::Client,
    state: RwLock<State>,
}

impl JwksCache {
    pub fn new(url: String, http: reqwest::Client) -> Self {
        Self { url, http, state: RwLock::new(State::default()) }
    }

    pub async fn key(&self, kid: &str) -> AppResult<DecodingKey> {
        {
            let s = self.state.read().await;
            if s.fetched.is_some_and(|t| t.elapsed() < TTL) {
                if let Some(k) = s.keys.get(kid) {
                    return Ok(k.clone());
                }
            }
        }
        let mut s = self.state.write().await;
        let stale = s.fetched.is_none_or(|t| t.elapsed() >= TTL);
        let may_refresh = s.fetched.is_none_or(|t| t.elapsed() >= MIN_REFRESH);
        if stale || (!s.keys.contains_key(kid) && may_refresh) {
            match self.fetch().await {
                Ok(keys) => {
                    *s = State { keys, fetched: Some(Instant::now()) };
                }
                Err(e) if s.keys.is_empty() => return Err(e),
                Err(e) => tracing::warn!(url = %self.url, error = %e, "JWKS refresh failed; using cached keys"),
            }
        }
        s.keys.get(kid).cloned().ok_or_else(|| AppError::unauthenticated("Unknown signing key"))
    }

    async fn fetch(&self) -> AppResult<HashMap<String, DecodingKey>> {
        let unavailable = |e: reqwest::Error| {
            tracing::warn!(url = %self.url, error = %e, "JWKS fetch failed");
            AppError::unavailable("Sign-in provider is unreachable. Please try again.")
        };
        let jwks: Jwks =
            self.http.get(&self.url).send().await.map_err(unavailable)?.error_for_status().map_err(unavailable)?.json().await.map_err(unavailable)?;
        let mut keys = HashMap::new();
        for k in jwks.keys {
            if let (Some(kid), "RSA", Some(n), Some(e)) = (k.kid, k.kty.as_str(), k.n, k.e) {
                if let Ok(key) = DecodingKey::from_rsa_components(&n, &e) {
                    keys.insert(kid, key);
                }
            }
        }
        tracing::debug!(url = %self.url, count = keys.len(), "JWKS refreshed");
        Ok(keys)
    }
}
