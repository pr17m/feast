//! `feast` — one binary, several roles. Run everything in one process for
//! development, or split roles across processes/machines in production:
//!
//! ```text
//! Every deployment serves one country: `--country <cc>` or `FEAST_COUNTRY`.
//!
//! feast --country in         # = feast all, for India
//! feast gateway              # HTTP API + live updates (scale horizontally)
//! feast worker               # outbox relay + notification projector (scale horizontally)
//! feast notification         # notification gRPC service
//! feast migrate              # apply database migrations and exit
//! feast seed [--reset]       # load demo data
//! feast markets sync         # re-apply config/countries/<cc>.yaml to the database
//! feast config               # print effective config (secrets masked)
//! ```

mod roles;
mod settings;
mod wiring;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "feast", version, about = "Feast backend")]
struct Cli {
    /// Directory containing default.toml (and <env>.toml|yaml, local.toml|yaml).
    #[arg(long, global = true, default_value = "config", env = "FEAST_CONFIG_DIR")]
    config: String,
    /// Environment name: selects config/<env>.toml|yaml (default: FEAST_ENV or "development").
    #[arg(long, global = true)]
    env: Option<String>,
    /// Country this deployment serves (ISO code, e.g. in, au): loads config/countries/<cc>.yaml.
    #[arg(long, global = true, env = "FEAST_COUNTRY")]
    country: Option<String>,
    #[command(subcommand)]
    role: Option<Role>,
}

#[derive(Subcommand, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Gateway + worker (+ notification service) in one process.
    All,
    /// HTTP API and Server-Sent Events for the apps.
    Gateway,
    /// Background processing: outbox relay and notification projector.
    Worker,
    /// Notification gRPC service.
    Notification,
    /// Apply database migrations and exit.
    Migrate,
    /// Load demo data.
    Seed {
        /// Wipe all data first.
        #[arg(long)]
        reset: bool,
        /// Only refresh demo photos on restaurants already in the database (keeps all other data).
        #[arg(long, conflicts_with = "reset")]
        photos: bool,
    },
    /// Country market admin.
    Markets {
        #[command(subcommand)]
        action: MarketsAction,
    },
    /// Print the effective configuration with secrets masked.
    Config,
}

#[derive(Subcommand, Clone, Copy, PartialEq, Eq)]
enum MarketsAction {
    /// Overwrite the market and cities in the database with the country file.
    Sync,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let settings = settings::Settings::load(&cli.config, cli.env.as_deref(), cli.country.as_deref())?;
    roles::init_tracing(&settings);
    match cli.role.unwrap_or(Role::All) {
        Role::All => roles::all(settings).await,
        Role::Gateway => roles::gateway(settings).await,
        Role::Worker => roles::worker(settings).await,
        Role::Notification => roles::notification(settings).await,
        Role::Migrate => roles::migrate(settings).await,
        Role::Seed { reset, photos } => roles::seed(settings, reset, photos).await,
        Role::Markets { action: MarketsAction::Sync } => roles::markets_sync(settings).await,
        Role::Config => {
            println!("{}", settings.redacted_toml());
            Ok(())
        }
    }
}
