//! Who is acting right now, for the audit trail.
//!
//! The delivery layer opens a scope per request ([`scope`]) with the request
//! id and client IP; signing in fills in the user ([`set_user`]); background
//! jobs open their own scope (`Actor::system("sweeper")`). Every domain event
//! built inside the scope carries the actor (`Feast::event`), so audit records
//! written with the event know who caused the change — without threading the
//! caller through every use case.

use std::future::Future;
use std::sync::{Arc, Mutex};

use feast_domain::audit::{Actor, ActorKind};
use feast_domain::model::User;

tokio::task_local! {
    static CURRENT: Arc<Mutex<Actor>>;
}

/// Run `f` with `actor` as the current actor.
pub async fn scope<F: Future>(actor: Actor, f: F) -> F::Output {
    CURRENT.scope(Arc::new(Mutex::new(actor)), f).await
}

/// The current actor; `Feast · unknown` outside any scope.
pub fn current() -> Actor {
    CURRENT.try_with(|a| a.lock().map(|g| g.clone()).unwrap_or_default()).unwrap_or_else(|_| Actor::system("unknown"))
}

/// The request is now known to be made by `user`.
pub fn set_user(user: &User) {
    let _ = CURRENT.try_with(|a| {
        if let Ok(mut g) = a.lock() {
            g.kind = ActorKind::User;
            g.user_id = Some(user.id.clone());
            g.role = Some(user.role);
            g.name = Some(user.name.clone());
            g.source = None;
        }
    });
}

/// The request is a provider webhook (no signed-in user).
pub fn set_webhook(provider: &str) {
    let _ = CURRENT.try_with(|a| {
        if let Ok(mut g) = a.lock() {
            g.kind = ActorKind::Webhook;
            g.source = Some(provider.to_owned());
        }
    });
}

/// An anonymous request's actor (before any sign-in).
pub fn request(ip: Option<String>, request_id: Option<String>) -> Actor {
    Actor { kind: ActorKind::Anonymous, ip, request_id, ..Actor::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use feast_domain::model::Role;

    #[tokio::test]
    async fn scoped_actor_picks_up_the_signed_in_user() {
        assert_eq!(current().kind, ActorKind::System);
        let seen = scope(request(Some("1.2.3.4".into()), Some("req-1".into())), async {
            let u = User {
                id: "u_1".into(),
                name: "Asha".into(),
                phone: None,
                email: None,
                role: Role::Owner,
                restaurant_ids: vec![],
                status: feast_domain::model::UserStatus::Active,
                created_at: "2026-10-25T00:00:00Z".into(),
            };
            set_user(&u);
            current()
        })
        .await;
        assert_eq!(seen.kind, ActorKind::User);
        assert_eq!(seen.user_id.as_deref(), Some("u_1"));
        assert_eq!(seen.role, Some(Role::Owner));
        assert_eq!(seen.ip.as_deref(), Some("1.2.3.4"));
        assert_eq!(seen.request_id.as_deref(), Some("req-1"));
    }
}
