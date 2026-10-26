//! Sign-in: phone OTP or social login (Google/Apple/Facebook) for diners,
//! email + password for partners and admins.

use feast_domain::model::{AuthProvider, AuthProviderInfo, AuthSession, OAuthLoginRequest, Role, User, UserStatus};

use crate::error::ErrorKind;
use crate::ports::ExternalIdentity;

use crate::context::Feast;
use crate::error::{AppError, AppResult};

impl Feast {
    pub async fn request_otp(&self, phone: &str) -> AppResult<Option<String>> {
        if !self.is_valid_phone(phone).await? {
            return Err(AppError::validation("INVALID_PHONE", "Enter a valid mobile number"));
        }
        self.p.otp.send(phone).await
    }

    /// Verify an OTP and sign in, creating a CUSTOMER account on first sign-in.
    pub async fn verify_otp(&self, phone: &str, otp: &str, name: Option<&str>) -> AppResult<AuthSession> {
        if !self.is_valid_phone(phone).await? {
            return Err(AppError::validation("INVALID_PHONE", "Enter a valid mobile number"));
        }
        if !self.p.otp.verify(phone, otp).await? {
            return Err(AppError::new(crate::error::ErrorKind::Unauthenticated, "BAD_OTP", "That OTP is incorrect"));
        }
        let user = match self.p.users.find_by_phone(phone).await? {
            Some(u) if u.status == UserStatus::Blocked => {
                return Err(AppError::new(crate::error::ErrorKind::Forbidden, "BLOCKED", "This account has been blocked. Contact support."))
            }
            Some(u) => u,
            None => {
                let u = User {
                    id: self.p.ids.new_id("u"),
                    name: name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Feast diner").to_owned(),
                    phone: Some(phone.to_owned()),
                    email: None,
                    role: Role::Customer,
                    restaurant_ids: vec![],
                    status: UserStatus::Active,
                    created_at: self.p.clock.now_iso(),
                };
                self.p.users.insert(&u, None).await?;
                u
            }
        };
        Ok(AuthSession { token: self.p.tokens.issue(&user)?, user })
    }

    pub async fn login(&self, email: &str, password: &str) -> AppResult<AuthSession> {
        let bad = || AppError::new(crate::error::ErrorKind::Unauthenticated, "BAD_CREDENTIALS", "Email or password is incorrect");
        let (user, hash) = self.p.users.find_by_email(email.trim()).await?.ok_or_else(bad)?;
        let hash = hash.ok_or_else(bad)?;
        if !self.p.passwords.verify(password, &hash) {
            return Err(bad());
        }
        if user.status == UserStatus::Blocked {
            return Err(AppError::new(crate::error::ErrorKind::Forbidden, "BLOCKED", "This account has been blocked"));
        }
        Ok(AuthSession { token: self.p.tokens.issue(&user)?, user })
    }

    /// Resolve a bearer token to an active user (fresh from the DB, so role and
    /// restaurant links changed by an admin apply immediately).
    pub async fn authenticate(&self, token: &str) -> AppResult<User> {
        let claims = self.p.tokens.verify(token)?;
        match self.p.users.get(&claims.user_id).await? {
            Some(u) if u.is_active() => {
                crate::actor::set_user(&u);
                Ok(u)
            }
            Some(_) => Err(AppError::new(crate::error::ErrorKind::Forbidden, "BLOCKED", "This account has been blocked")),
            None => Err(AppError::unauthenticated("Please sign in again")),
        }
    }

    // ------------------------------------------------------------ social login

    /// Social providers the clients should offer (from config).
    pub fn auth_providers(&self) -> Vec<AuthProviderInfo> {
        self.p.identity.iter().map(|p| p.info()).collect()
    }

    /// Sign in with a credential from Google, Apple or Facebook.
    ///
    /// 1. Identity already linked → that user.
    /// 2. Provider-verified email matches a **customer** → link and sign in.
    ///    Partner/admin accounts are never linked by email alone (account
    ///    takeover risk); they keep using their password.
    /// 3. Otherwise → new CUSTOMER account, linked to the identity.
    pub async fn oauth_login(&self, provider: AuthProvider, req: &OAuthLoginRequest) -> AppResult<AuthSession> {
        let idp = self
            .p
            .identity
            .iter()
            .find(|p| p.info().provider == provider)
            .ok_or_else(|| AppError::new(ErrorKind::NotFound, "PROVIDER_DISABLED", format!("Sign in with {} isn't available", provider.label())))?;
        let ext = idp.verify(req).await?;

        if let Some(user) = self.p.users.find_by_identity(provider, &ext.subject).await? {
            return self.session_for(user);
        }

        let verified_email = ext.email.as_deref().filter(|_| ext.email_verified).map(|e| e.trim().to_lowercase());
        if let Some(email) = &verified_email {
            if let Some((user, _)) = self.p.users.find_by_email(email).await? {
                if user.role != Role::Customer {
                    return Err(AppError::new(
                        ErrorKind::Forbidden,
                        "USE_PASSWORD",
                        "This email belongs to a partner account. Sign in with your email and password.",
                    ));
                }
                self.ensure_active(&user)?;
                self.p.users.link_identity(&user.id, &ext).await?;
                tracing::info!(user = %user.id, provider = provider.as_str(), "linked social identity by verified email");
                return self.session_for(user);
            }
        }

        let user = User {
            id: self.p.ids.new_id("u"),
            name: display_name(&ext, req),
            phone: None,
            email: verified_email,
            role: Role::Customer,
            restaurant_ids: vec![],
            status: UserStatus::Active,
            created_at: self.p.clock.now_iso(),
        };
        self.p.users.insert(&user, None).await?;
        self.p.users.link_identity(&user.id, &ext).await?;
        tracing::info!(user = %user.id, provider = provider.as_str(), "new customer via social login");
        self.session_for(user)
    }

    /// Social providers linked to the caller's account.
    pub async fn my_identities(&self, user: &User) -> AppResult<Vec<AuthProvider>> {
        self.p.users.identities_of(&user.id).await
    }

    fn ensure_active(&self, user: &User) -> AppResult<()> {
        if user.status == UserStatus::Blocked {
            return Err(AppError::new(ErrorKind::Forbidden, "BLOCKED", "This account has been blocked. Contact support."));
        }
        Ok(())
    }

    fn session_for(&self, user: User) -> AppResult<AuthSession> {
        self.ensure_active(&user)?;
        Ok(AuthSession { token: self.p.tokens.issue(&user)?, user })
    }
}

fn display_name(ext: &ExternalIdentity, req: &OAuthLoginRequest) -> String {
    [ext.name.as_deref(), req.name.as_deref()]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|n| !n.is_empty())
        .map(|n| n.chars().take(80).collect())
        .or_else(|| ext.email.as_deref().and_then(|e| e.split('@').next()).filter(|s| !s.is_empty()).map(str::to_owned))
        .unwrap_or_else(|| "Feast diner".to_owned())
}
