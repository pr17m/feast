//! What each role runs, plus graceful shutdown.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use feast_api::{router, ApiConfig, AppState, ReadinessCheck, RealtimeHub};
use feast_application::processes::{run_notification_projector, run_outbox_relay, run_reminder_sweeper, RelayConfig};
use tokio::sync::watch;
use tokio::task::JoinSet;
use tracing_subscriber::EnvFilter;

use crate::settings::Settings;
use crate::wiring::{self, BusCheck, DbCheck, Infra, NotificationCheck, Notifications, StorageCheck};

pub fn init_tracing(s: &Settings) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&s.log.level));
    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_target(false);
    if s.log.format == "json" {
        builder.json().flatten_event(true).init();
    } else {
        builder.init();
    }
}

/// Flips to `true` on Ctrl-C or SIGTERM.
fn shutdown_signal() -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        let ctrl_c = tokio::signal::ctrl_c();
        #[cfg(unix)]
        let term = async {
            if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                s.recv().await;
            } else {
                std::future::pending::<()>().await;
            }
        };
        #[cfg(not(unix))]
        let term = std::future::pending::<()>();
        tokio::select! {
            _ = ctrl_c => {},
            _ = term => {},
        }
        tracing::info!("shutdown requested");
        let _ = tx.send(true);
    });
    rx
}

async fn wait(mut rx: watch::Receiver<bool>) {
    let _ = rx.wait_for(|v| *v).await;
}

fn warn_if_memory_bus(inf: &Infra, role: &str) {
    if inf.settings.bus.kind == "memory" {
        tracing::warn!(role, "bus.kind=memory only connects components inside ONE process; use bus.kind=redis when running roles separately");
    }
}

// ------------------------------------------------------------ roles

pub async fn all(settings: Settings) -> Result<()> {
    let inf = wiring::infra(settings).await?;
    let shutdown = shutdown_signal();
    let mut tasks = JoinSet::new();
    // In grpc mode, run the notification service here too, so `feast all`
    // exercises the real gRPC path end to end.
    if inf.settings.notifications.mode == "grpc" {
        let svc = wiring::notification_service(&inf).await?;
        tasks.spawn(notification_server(inf.settings.clone(), svc, shutdown.clone()));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let core = wiring::core(&inf, "feast-all").await?;
    wiring::seed_if_empty(&inf, &core).await?;
    spawn_worker(&inf, &core, &mut tasks, shutdown.clone());
    tasks.spawn(http_server(inf, core, shutdown.clone()));
    join(tasks).await
}

pub async fn gateway(settings: Settings) -> Result<()> {
    let inf = wiring::infra(settings).await?;
    warn_if_memory_bus(&inf, "gateway");
    let core = wiring::core(&inf, "feast-gateway").await?;
    wiring::seed_if_empty(&inf, &core).await?;
    let mut tasks = JoinSet::new();
    tasks.spawn(http_server(inf, core, shutdown_signal()));
    join(tasks).await
}

pub async fn worker(settings: Settings) -> Result<()> {
    let inf = wiring::infra(settings).await?;
    warn_if_memory_bus(&inf, "worker");
    let core = wiring::core(&inf, "feast-worker").await?;
    let mut tasks = JoinSet::new();
    let shutdown = shutdown_signal();
    spawn_worker(&inf, &core, &mut tasks, shutdown.clone());
    tasks.spawn(health_server(inf.settings.clone(), core.repos.admin.clone(), inf.bus.clone(), shutdown));
    join(tasks).await
}

/// Worker health endpoints (`/healthz`, `/readyz`) on http.port, for platforms
/// that probe every container (ECS, Container Apps, Cloud Run, Kubernetes).
async fn health_server(
    s: Settings,
    db: Arc<dyn feast_infra::database::DatabaseAdmin>,
    bus: Arc<dyn feast_application::ports::MessageBus>,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    use axum::routing::get;
    let ready = move || {
        let (db, bus) = (db.clone(), bus.clone());
        async move {
            match (db.ping().await, bus.health().await) {
                (Ok(()), Ok(())) => (axum::http::StatusCode::OK, "ok"),
                _ => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "degraded"),
            }
        }
    };
    let app = axum::Router::new().route("/healthz", get(|| async { "ok" })).route("/readyz", get(ready));
    let addr: SocketAddr = format!("{}:{}", s.http.host, s.http.port).parse().context("http.host/http.port")?;
    let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, "worker health endpoints listening");
    axum::serve(listener, app).with_graceful_shutdown(wait(shutdown)).await.context("worker health server")?;
    Ok(())
}

pub async fn notification(settings: Settings) -> Result<()> {
    let inf = wiring::infra(settings).await?;
    let svc = wiring::notification_service(&inf).await?;
    notification_server(inf.settings.clone(), svc, shutdown_signal()).await
}

pub async fn migrate(settings: Settings) -> Result<()> {
    let inf = wiring::infra(settings).await?;
    // Connecting runs the embedded migrations for both databases.
    let core = wiring::core(&inf, "feast-migrate").await?;
    if let Notifications::Grpc(_) = core.notifications {
        wiring::notification_service(&inf).await?;
    }
    tracing::info!("migrations applied");
    Ok(())
}

pub async fn seed(settings: Settings, reset: bool, photos: bool) -> Result<()> {
    let inf = wiring::infra(settings).await?;
    let core = wiring::core(&inf, "feast-seed").await?;
    if photos {
        let passwords = Arc::new(feast_infra::auth::Argon2Hasher::default());
        wiring::seeder(&inf, &core.repos, passwords, false).refresh_photos().await.map_err(|e| anyhow::anyhow!("{e}"))?;
        return Ok(());
    }
    if reset {
        core.repos.admin.wipe().await.map_err(|e| anyhow::anyhow!("{e}"))?;
    } else if !core.repos.admin.is_empty().await.map_err(|e| anyhow::anyhow!("{e}"))? {
        anyhow::bail!("database already has data; use `feast seed --reset` to wipe and reload");
    }
    let passwords = Arc::new(feast_infra::auth::Argon2Hasher::default());
    wiring::seeder(&inf, &core.repos, passwords, inf.settings.app.seed_history).run().await.map_err(|e| anyhow::anyhow!("{e}"))?;
    tracing::info!("demo data loaded");
    Ok(())
}

/// `feast markets sync`: make the database's market and cities match the
/// country file again (after editing config/countries/<cc>.yaml).
pub async fn markets_sync(settings: Settings) -> Result<()> {
    let inf = wiring::infra(settings).await?;
    let core = wiring::core(&inf, "feast-markets").await?;
    feast_infra::seed::ensure_market(&core.repos, &inf.country, true).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    tracing::info!(country = %inf.country.market.country_code, "market synced from country file");
    Ok(())
}

// ------------------------------------------------------------ building blocks

fn spawn_worker(inf: &Infra, core: &wiring::Core, tasks: &mut JoinSet<Result<()>>, shutdown: watch::Receiver<bool>) {
    let s = &inf.settings;
    let relay = run_outbox_relay(
        core.repos.outbox.clone(),
        inf.bus.clone(),
        RelayConfig { poll_interval: Duration::from_millis(s.outbox.poll_interval_ms), batch_size: s.outbox.batch_size },
        shutdown.clone(),
    );
    tasks.spawn(async move {
        relay.await;
        Ok(())
    });
    let consumer = format!("{}-{}", hostname(), std::process::id());
    let projector = run_notification_projector(core.feast.clone(), inf.bus.clone(), s.bus.consumer_group.clone(), consumer, shutdown.clone());
    tasks.spawn(async move {
        projector.await;
        Ok(())
    });
    let sweeper = run_reminder_sweeper(core.feast.clone(), Duration::from_secs(s.subscriptions.reminder_interval_secs.max(60)), shutdown);
    tasks.spawn(async move {
        sweeper.await;
        Ok(())
    });
}

async fn http_server(inf: Infra, core: wiring::Core, shutdown: watch::Receiver<bool>) -> Result<()> {
    let s = &inf.settings;
    let hub = Arc::new(RealtimeHub::new(s.bus.memory_capacity));
    tokio::spawn(hub.clone().run(inf.bus.clone(), shutdown.clone()));
    let ready: Vec<Arc<dyn ReadinessCheck>> = vec![
        Arc::new(DbCheck(core.repos.admin.clone())),
        Arc::new(BusCheck(inf.bus.clone())),
        Arc::new(NotificationCheck(core.notifications.gateway())),
        Arc::new(StorageCheck(inf.files.clone())),
    ];
    let state = AppState {
        feast: core.feast.clone(),
        hub: hub.clone(),
        ready: Arc::new(ready),
        proxy_hops: s.http.proxy_hops,
        metrics: Arc::new(feast_api::Metrics::default()),
        metrics_token: Some(s.http.metrics_token.clone()).filter(|t| !t.is_empty()),
        limiter: Arc::new(feast_api::RateLimiter::new(s.http.auth_rate_limit_per_minute)),
    };
    let app = router(
        state,
        &ApiConfig {
            cors_origins: s.http.cors_origins.clone(),
            request_timeout: Duration::from_secs(s.http.request_timeout_secs),
            max_body_bytes: s.http.max_body_bytes,
            max_upload_bytes: s.http.max_upload_bytes,
        },
    );
    let addr: SocketAddr = format!("{}:{}", s.http.host, s.http.port).parse().context("http.host/http.port")?;
    let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, country = %s.country(), env = %s.app.env, db = %scheme(&s.database.url), bus = %s.bus.kind, notifications = %s.notifications.mode, "HTTP API listening");
    let stop = shutdown.clone();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move {
            wait(stop).await;
            hub.close(); // end SSE streams so in-flight requests can drain
        })
        .await
        .context("http server")?;
    tracing::info!("HTTP API stopped");
    Ok(())
}

async fn notification_server(settings: Settings, svc: Arc<feast_notification::NotificationService>, shutdown: watch::Receiver<bool>) -> Result<()> {
    let n = &settings.notifications;
    let addr: SocketAddr = n.grpc_listen.parse().context("notifications.grpc_listen")?;
    let listener = tokio::net::TcpListener::bind(addr).await.with_context(|| format!("binding {addr}"))?;
    feast_notification::serve(listener, svc, Some(n.service_token.clone()), wait(shutdown)).await.context("notification gRPC server")?;
    tracing::info!("notification service stopped");
    Ok(())
}

async fn join(mut tasks: JoinSet<Result<()>>) -> Result<()> {
    let mut first_err = None;
    while let Some(r) = tasks.join_next().await {
        match r {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                tracing::error!(error = %format!("{e:#}"), "component failed");
                first_err.get_or_insert(e);
            }
            Err(e) => {
                first_err.get_or_insert(anyhow::anyhow!("task panicked: {e}"));
            }
        }
    }
    first_err.map_or(Ok(()), Err)
}

fn scheme(url: &str) -> &str {
    url.split(':').next().unwrap_or("?")
}

fn hostname() -> String {
    std::env::var("HOSTNAME").ok().filter(|h| !h.is_empty()).unwrap_or_else(|| "local".into())
}
