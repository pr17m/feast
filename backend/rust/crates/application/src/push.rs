//! Push notifications to the phone apps: device registration, the on/off
//! switch, and delivery of every stored notification to the user's phones.

use feast_domain::model::{Notification, Role, User};
use feast_domain::push::{push_backoff_ms, valid_token, PushApp, PushMessage, PushOutcome, PushSettings, PushToken, PushTokenRequest, PUSH_MAX_ATTEMPTS};

use crate::context::Feast;
use crate::error::{AppError, AppResult};
use crate::ports::{PushSender, PushTokenRepository};

impl Feast {
    /// `POST /v1/me/push-tokens`: the app registers (or refreshes) this phone.
    pub async fn register_push_token(&self, user: &User, req: &PushTokenRequest) -> AppResult<PushToken> {
        let token = req.token.trim();
        if !valid_token(token) {
            return Err(AppError::validation("INVALID_PUSH_TOKEN", "That push token isn't valid"));
        }
        // The partner app is for restaurant staff; diners use the Feast app.
        if req.app == PushApp::Partner && user.role == Role::Customer {
            return Err(AppError::forbidden("Feast Partner is for restaurant accounts"));
        }
        let now = self.p.clock.now_iso();
        let t =
            PushToken { token: token.to_owned(), user_id: user.id.clone(), platform: req.platform, app: req.app, created_at: now.clone(), last_seen_at: now };
        self.p.push_tokens.upsert_token(&t).await?;
        Ok(t)
    }

    /// `DELETE /v1/me/push-tokens/{token}`: on sign-out. Idempotent.
    pub async fn unregister_push_token(&self, user: &User, token: &str) -> AppResult<bool> {
        self.p.push_tokens.delete_token(&user.id, token.trim()).await
    }

    pub async fn push_settings(&self, user: &User) -> AppResult<PushSettings> {
        Ok(PushSettings { enabled: self.p.push_tokens.push_enabled(&user.id).await? })
    }

    pub async fn set_push_settings(&self, user: &User, s: &PushSettings) -> AppResult<PushSettings> {
        self.p.push_tokens.set_push_enabled(&user.id, s.enabled).await?;
        Ok(PushSettings { enabled: s.enabled })
    }

    /// Push one stored notification to its user's phones (run by the worker
    /// for every `notification.created`). Returns how many phones got it.
    /// A failed push never fails the notification; see [`push_to_phones`].
    pub async fn deliver_push(&self, n: &Notification) -> AppResult<usize> {
        push_to_phones(self.p.push_tokens.as_ref(), self.p.push.as_ref(), n, self.p.clock.now_ms()).await
    }

    /// Sweeper: send again the pushes that failed for a passing reason.
    pub async fn retry_pushes(&self) -> AppResult<usize> {
        retry_due_pushes(self.p.push_tokens.as_ref(), self.p.push.as_ref(), self.p.clock.now_ms(), 100).await
    }
}

/// How long one worker owns a push it is sending (if it dies, the sweeper
/// picks the push up after this).
pub const PUSH_CLAIM_MS: i64 = 60_000;

/// Push `n` to each of its user's phones, once per phone across all workers
/// (`notification.created` may be delivered more than once):
/// - sent → done; never sent again;
/// - token gone (uninstalled) → token dropped;
/// - passing failure (5xx, 429, timeout) → retried by the sweeper with backoff,
///   up to `PUSH_MAX_ATTEMPTS`, then given up (logged);
/// - permanent failure → logged, not retried.
pub async fn push_to_phones(repo: &dyn PushTokenRepository, sender: &dyn PushSender, n: &Notification, now_ms: i64) -> AppResult<usize> {
    if !sender.enabled() || !repo.push_enabled(&n.user_id).await? {
        return Ok(0);
    }
    let mut sent = 0;
    for t in repo.tokens_for(&n.user_id).await? {
        if !repo.claim_push(n, &t.token, now_ms + PUSH_CLAIM_MS).await? {
            tracing::debug!(notification = %n.id, "push already handled by another worker");
            continue;
        }
        sent += send_one(repo, sender, n, &t, 1, now_ms).await? as usize;
    }
    tracing::info!(notification = %n.id, sent, "push delivered");
    Ok(sent)
}

/// Retry pushes whose time has come (each claimed by one caller). Returns how many were sent.
pub async fn retry_due_pushes(repo: &dyn PushTokenRepository, sender: &dyn PushSender, now_ms: i64, limit: u32) -> AppResult<usize> {
    if !sender.enabled() {
        return Ok(0);
    }
    let mut sent = 0;
    for d in repo.claim_due_pushes(now_ms, now_ms + PUSH_CLAIM_MS, limit).await? {
        let n = &d.notification;
        let token = repo.tokens_for(&n.user_id).await?.into_iter().find(|t| t.token == d.token);
        let Some(t) = token else {
            repo.finish_push(&n.id, &d.token, "DROPPED", 0, Some("token no longer registered")).await?;
            continue;
        };
        if !repo.push_enabled(&n.user_id).await? {
            repo.finish_push(&n.id, &d.token, "FAILED", 0, Some("push turned off")).await?;
            continue;
        }
        sent += send_one(repo, sender, n, &t, d.attempts, now_ms).await? as usize;
    }
    Ok(sent)
}

async fn send_one(repo: &dyn PushTokenRepository, sender: &dyn PushSender, n: &Notification, t: &PushToken, attempts: i64, now_ms: i64) -> AppResult<bool> {
    match sender.send(t, &PushMessage::from_notification(n)).await {
        PushOutcome::Sent => {
            repo.finish_push(&n.id, &t.token, "SENT", 0, None).await?;
            Ok(true)
        }
        PushOutcome::Unregistered => {
            tracing::info!(user = %n.user_id, platform = t.platform.as_str(), "push token no longer valid; removed");
            repo.forget_token(&t.token).await?;
            repo.finish_push(&n.id, &t.token, "DROPPED", 0, Some("unregistered")).await?;
            Ok(false)
        }
        PushOutcome::Retry(e) if attempts < PUSH_MAX_ATTEMPTS => {
            let next = now_ms + push_backoff_ms(attempts);
            tracing::warn!(user = %n.user_id, notification = %n.id, attempts, error = %e, "push failed; will retry");
            repo.finish_push(&n.id, &t.token, "RETRY", next, Some(&e)).await?;
            Ok(false)
        }
        PushOutcome::Retry(e) => {
            tracing::error!(user = %n.user_id, notification = %n.id, attempts, error = %e, "push failed; giving up");
            repo.finish_push(&n.id, &t.token, "GAVE_UP", 0, Some(&e)).await?;
            Ok(false)
        }
        PushOutcome::Failed(e) => {
            tracing::warn!(user = %n.user_id, notification = %n.id, error = %e, "push failed (not retried)");
            repo.finish_push(&n.id, &t.token, "FAILED", 0, Some(&e)).await?;
            Ok(false)
        }
    }
}
