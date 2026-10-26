//! Message bus adapters.
//!
//! - `memory`: in-process (tokio channels). Single-node / dev / tests.
//! - `redis`: Redis Streams with consumer groups. Durable, multi-node.
//!
//! Kafka, NATS JetStream or RabbitMQ would be one more module implementing
//! `MessageBus` + `Subscription`; select it with `bus.kind` in config.

pub mod memory;
#[cfg(feature = "redis")]
pub mod redis;

use std::sync::Arc;

use feast_application::ports::MessageBus;
#[allow(unused_imports)]
use feast_application::{AppError, AppResult};

#[derive(Debug, Clone)]
pub enum BusConfig {
    Memory { capacity: usize },
    Redis(RedisBusConfig),
}

#[derive(Debug, Clone)]
pub struct RedisBusConfig {
    pub url: String,
    /// Stream name, e.g. `feast:events`.
    pub stream: String,
    /// Approximate cap on stream length (older entries are trimmed).
    pub max_len: usize,
    /// How long a blocking read waits before looping (ms).
    pub block_ms: usize,
    /// Messages pending longer than this on a dead consumer are claimed by a live one (ms).
    pub claim_idle_ms: usize,
}

pub async fn connect(cfg: &BusConfig) -> AppResult<Arc<dyn MessageBus>> {
    match cfg {
        BusConfig::Memory { capacity } => Ok(Arc::new(memory::InMemoryBus::new(*capacity))),
        #[cfg(feature = "redis")]
        BusConfig::Redis(c) => Ok(Arc::new(redis::RedisStreamsBus::connect(c.clone()).await?)),
        #[cfg(not(feature = "redis"))]
        BusConfig::Redis(_) => Err(AppError::validation("UNSUPPORTED_BUS", "built without the `redis` feature")),
    }
}
