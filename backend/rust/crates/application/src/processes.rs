//! Long-running background processes (run by the `worker` role).
//!
//! - **Outbox relay**: reads events committed to the outbox table and publishes
//!   them to the message bus, in order. One relay publishes at a time (a
//!   database lease), so several workers never publish the same rows twice in
//!   the normal case; consumers are idempotent by event id for the rest.
//! - **Notification projector**: consumes the bus as a competing-consumer group
//!   and turns events into notifications (via the notification service).
//!
//! Both stop cleanly when the shutdown signal flips to `true`.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

use crate::context::Feast;
use crate::ports::{Clock, MessageBus, OutboxRepository, SharedStateRepository, SubscriptionMode};

pub type Shutdown = watch::Receiver<bool>;

pub struct RelayConfig {
    pub poll_interval: Duration,
    pub batch_size: u32,
}

/// A named lease in the shared database: at most one holder at a time across
/// every worker process. The holder renews it on each tick; if it dies, the
/// lease expires after `ttl` and another worker takes over.
#[derive(Clone)]
pub struct Lease {
    pub store: Arc<dyn SharedStateRepository>,
    pub clock: Arc<dyn Clock>,
    pub name: &'static str,
    pub holder: String,
    pub ttl: Duration,
}

impl Lease {
    /// Take or renew the lease. A database error counts as "not held" (fail
    /// closed: better a short pause than two holders).
    pub async fn hold(&self) -> bool {
        let now = self.clock.now_ms();
        match self.store.lease(self.name, &self.holder, now, now + self.ttl.as_millis() as i64).await {
            Ok(held) => held,
            Err(e) => {
                tracing::warn!(lease = self.name, error = %e, "lease check failed");
                false
            }
        }
    }
}

pub async fn run_outbox_relay(outbox: Arc<dyn OutboxRepository>, bus: Arc<dyn MessageBus>, cfg: RelayConfig, lease: Lease, mut shutdown: Shutdown) {
    tracing::info!(holder = %lease.holder, "outbox relay started");
    let mut leader = false;
    loop {
        if *shutdown.borrow() {
            break;
        }
        let held = lease.hold().await;
        if held != leader {
            leader = held;
            tracing::info!(holder = %lease.holder, leader, "outbox relay leadership changed");
        }
        if !held {
            tokio::select! {
                _ = tokio::time::sleep(cfg.poll_interval.max(Duration::from_millis(500))) => {}
                _ = shutdown.changed() => {}
            }
            continue;
        }
        let mut published = 0usize;
        match outbox.fetch_pending(cfg.batch_size).await {
            Ok(batch) => {
                let mut done = Vec::with_capacity(batch.len());
                for (seq, env) in batch {
                    match bus.publish(&env).await {
                        Ok(()) => done.push(seq),
                        Err(e) => {
                            // Stop here to preserve order; retry on the next tick.
                            tracing::warn!(seq, event = env.type_name(), error = %e, "publish failed; will retry");
                            let _ = outbox.record_failure(seq, &e.to_string()).await;
                            break;
                        }
                    }
                }
                published = done.len();
                if !done.is_empty() {
                    if let Err(e) = outbox.mark_published(&done).await {
                        tracing::error!(error = %e, "failed to mark outbox rows published (they will be re-sent)");
                    }
                }
            }
            Err(e) => tracing::error!(error = %e, "outbox fetch failed"),
        }
        if published as u32 == cfg.batch_size {
            continue; // more waiting — drain without sleeping
        }
        tokio::select! {
            _ = tokio::time::sleep(cfg.poll_interval) => {}
            _ = shutdown.changed() => {}
        }
    }
    tracing::info!("outbox relay stopped");
}

pub async fn run_notification_projector(feast: Arc<Feast>, bus: Arc<dyn MessageBus>, group: String, consumer: String, mut shutdown: Shutdown) {
    let mut sub = loop {
        match bus.subscribe(SubscriptionMode::Group { group: group.clone(), consumer: consumer.clone() }).await {
            Ok(s) => break s,
            Err(e) => {
                tracing::error!(error = %e, "projector: subscribe failed; retrying in 2s");
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(2)) => continue,
                    _ = shutdown.changed() => return,
                }
            }
        }
    };
    tracing::info!(%group, %consumer, "notification projector started");
    loop {
        let next = tokio::select! {
            n = sub.next() => n,
            _ = shutdown.changed() => break,
        };
        let delivery = match next {
            Ok(Some(d)) => d,
            Ok(None) => break,
            Err(e) => {
                tracing::error!(error = %e, "projector: receive failed");
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        // Retry transient failures; give up after a few attempts so one bad
        // message can't block the stream (it stays in the logs for replay).
        let mut attempt = 0;
        loop {
            attempt += 1;
            match feast.project_notification(&delivery.envelope).await {
                Ok(n) => {
                    if n > 0 {
                        tracing::info!(event = delivery.envelope.type_name(), id = %delivery.envelope.id, notified = n, "notifications created");
                    }
                    break;
                }
                Err(e) if attempt < 5 => {
                    tracing::warn!(attempt, error = %e, "projector: handling failed; retrying");
                    tokio::time::sleep(Duration::from_millis(200 * attempt)).await;
                }
                Err(e) => {
                    tracing::error!(event = delivery.envelope.type_name(), id = %delivery.envelope.id, error = %e, "projector: giving up on event (dead-letter)");
                    break;
                }
            }
        }
        if let Err(e) = sub.ack(&delivery).await {
            tracing::warn!(error = %e, "projector: ack failed");
        }
    }
    tracing::info!("notification projector stopped");
}

/// Reminders: trial/subscription ending (7, 3, 1 days before) and licences
/// expiring (30, 7, 1 days before). Runs periodically; each reminder's event
/// id is deterministic, so re-runs (or several workers) never notify twice.
pub async fn run_reminder_sweeper(feast: Arc<Feast>, every: Duration, holder: String, mut shutdown: Shutdown) {
    tracing::info!(?every, "reminder sweeper started");
    let lease = Lease { store: feast.p.shared.clone(), clock: feast.p.clock.clone(), name: "sweeper", holder, ttl: every + Duration::from_secs(30) };
    loop {
        // One worker sweeps at a time (each job is idempotent anyway; this
        // keeps provider calls, e.g. refund retries, from doubling up).
        if lease.hold().await {
            crate::actor::scope(feast_domain::audit::Actor::system("sweeper"), sweep_once(&feast)).await;
        }
        tokio::select! {
            _ = tokio::time::sleep(every) => {}
            _ = shutdown.changed() => break,
        }
    }
    tracing::info!("reminder sweeper stopped");
}

/// One pass of every periodic job (as the "sweeper" actor in the audit trail).
async fn sweep_once(feast: &Feast) {
    {
        match feast.sweep_reminders().await {
            Ok(n) if n > 0 => tracing::info!(reminders = n, "reminders queued"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "reminder sweep failed"),
        }
        match feast.sweep_cases().await {
            Ok(n) if n > 0 => tracing::info!(cases = n, "overdue grievance cases moved on"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "case sweep failed"),
        }
        match feast.sweep_offline_payments().await {
            Ok(n) if n > 0 => tracing::info!(payments = n, "unconfirmed counter payments expired"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "counter payment sweep failed"),
        }
        match feast.sweep_refunds().await {
            Ok(n) if n > 0 => tracing::info!(refunds = n, "pending refunds sent"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "refund sweep failed"),
        }
        match feast.retry_pushes().await {
            Ok(n) if n > 0 => tracing::info!(pushes = n, "failed pushes sent on retry"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "push retry sweep failed"),
        }
        match feast.p.shared.purge_expired(feast.p.clock.now_ms()).await {
            Ok(n) if n > 0 => tracing::debug!(rows = n, "expired rate-limit counters and codes removed"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "shared state cleanup failed"),
        }
        match feast.fill_missing_menu_photos().await {
            Ok(n) if n > 0 => tracing::info!(photos = n, "stock photos added to dishes without one"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "dish photo sweep failed"),
        }
    }
}
