//! Authentication adapters: JWT access tokens and Argon2id password hashing.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString};
use argon2::Argon2;
use feast_application::ports::{PasswordHasher, TokenClaims, TokenService};
use feast_application::{AppError, AppResult};
use feast_domain::model::{Role, User};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    role: String,
    iss: String,
    iat: i64,
    exp: i64,
}

/// HS256 JWTs signed with a shared secret (config `auth.jwt_secret`).
/// Stateless: any gateway instance can verify any token. Moving to RS256/JWKS
/// (or an external IdP) only changes this adapter.
pub struct JwtTokenService {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    issuer: String,
    ttl_secs: i64,
}

impl JwtTokenService {
    pub fn new(secret: &str, issuer: &str, ttl_secs: i64) -> Self {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[issuer]);
        validation.leeway = 30;
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            validation,
            issuer: issuer.to_owned(),
            ttl_secs,
        }
    }
}

impl TokenService for JwtTokenService {
    fn issue(&self, user: &User) -> AppResult<String> {
        let now = chrono::Utc::now().timestamp();
        let claims = Claims { sub: user.id.clone(), role: user.role.as_str().to_owned(), iss: self.issuer.clone(), iat: now, exp: now + self.ttl_secs };
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding).map_err(AppError::internal)
    }

    fn verify(&self, token: &str) -> AppResult<TokenClaims> {
        let data = decode::<Claims>(token, &self.decoding, &self.validation).map_err(|e| match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => AppError::unauthenticated("Your session has expired. Please sign in again."),
            _ => AppError::unauthenticated("Please sign in again."),
        })?;
        let role = Role::parse(&data.claims.role).ok_or_else(|| AppError::unauthenticated("Please sign in again."))?;
        Ok(TokenClaims { user_id: data.claims.sub, role })
    }
}

/// Argon2id with the crate's recommended defaults (memory-hard, salted).
#[derive(Default)]
pub struct Argon2Hasher {
    argon: Argon2<'static>,
}

impl PasswordHasher for Argon2Hasher {
    fn hash(&self, password: &str) -> AppResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        self.argon.hash_password(password.as_bytes(), &salt).map(|h| h.to_string()).map_err(AppError::internal)
    }

    fn verify(&self, password: &str, hash: &str) -> bool {
        PasswordHash::new(hash).map(|h| self.argon.verify_password(password.as_bytes(), &h).is_ok()).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use feast_domain::model::UserStatus;

    fn user() -> User {
        User {
            id: "u_1".into(),
            name: "Test".into(),
            phone: None,
            email: None,
            role: Role::Owner,
            restaurant_ids: vec![],
            status: UserStatus::Active,
            created_at: "2026-01-01T00:00:00.000Z".into(),
        }
    }

    #[test]
    fn jwt_roundtrip_and_rejects_tampering() {
        let svc = JwtTokenService::new("test-secret-that-is-long-enough-123", "feast", 3600);
        let t = svc.issue(&user()).unwrap();
        let c = svc.verify(&t).unwrap();
        assert_eq!((c.user_id.as_str(), c.role), ("u_1", Role::Owner));
        let other = JwtTokenService::new("another-secret-that-is-long-enough", "feast", 3600);
        assert!(other.verify(&t).is_err());
        let expired = JwtTokenService::new("test-secret-that-is-long-enough-123", "feast", -120);
        assert!(svc.verify(&expired.issue(&user()).unwrap()).is_err());
    }

    #[test]
    fn argon2_hash_verify() {
        let h = Argon2Hasher::default();
        let hash = h.hash("feast123").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(h.verify("feast123", &hash));
        assert!(!h.verify("wrong", &hash));
        assert!(!h.verify("feast123", "not-a-hash"));
    }
}
