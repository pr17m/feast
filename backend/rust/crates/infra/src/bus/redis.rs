//! Redis Streams message bus.
//!
//! One stream (`feast:events`) carries every domain event:
//!
//! - **Group** subscriptions use a consumer group (`XREADGROUP`): each event goes
//!   to one consumer in the group, must be `XACK`ed, and unacked events survive
//!   crashes. On start a consumer first re-reads its own pending entries, and it
//!   periodically claims entries stuck on dead consumers (`XAUTOCLAIM`).
//! - **Broadcast** subscriptions tail the stream with plain `XREAD` from "now",
//!   so every gateway instance sees every event (for live pushes to its clients).
//!
//! Each subscription owns a dedicated connection because blocking reads would
//! otherwise stall the shared, multiplexed publish connection.

use std::collections::VecDeque;

use async_trait::async_trait;
use feast_application::ports::{Delivery, MessageBus, Subscription, SubscriptionMode};
use feast_application::{AppError, AppResult};
use feast_domain::EventEnvelope;
use redis::aio::{ConnectionManager, MultiplexedConnection};
use redis::streams::{StreamId, StreamMaxlen, StreamReadOptions, StreamReadReply};
use redis::{AsyncCommands, Client, RedisError, Value};

use super::RedisBusConfig;

const BATCH: usize = 32;
/// Run XAUTOCLAIM every this many blocking reads.
const CLAIM_EVERY: u32 = 20;

fn bus_err(e: RedisError) -> AppError {
    tracing::warn!(error = %e, "redis error");
    AppError::unavailable("Message bus unavailable")
}

pub struct RedisStreamsBus {
    client: Client,
    conn: ConnectionManager,
    cfg: RedisBusConfig,
}

impl RedisStreamsBus {
    pub async fn connect(cfg: RedisBusConfig) -> AppResult<Self> {
        let client = Client::open(cfg.url.as_str()).map_err(|e| AppError::validation("BAD_REDIS_URL", e.to_string()))?;
        let conn = ConnectionManager::new(client.clone()).await.map_err(bus_err)?;
        tracing::info!(stream = %cfg.stream, "connected to Redis Streams bus");
        Ok(Self { client, conn, cfg })
    }
}

#[async_trait]
impl MessageBus for RedisStreamsBus {
    async fn publish(&self, event: &EventEnvelope) -> AppResult<()> {
        let mut c = self.conn.clone();
        let fields = [("type", event.type_name().to_owned()), ("id", event.id.clone()), ("payload", event.to_json())];
        let _: String = c.xadd_maxlen(&self.cfg.stream, StreamMaxlen::Approx(self.cfg.max_len), "*", &fields).await.map_err(bus_err)?;
        Ok(())
    }

    async fn health(&self) -> AppResult<()> {
        let mut c = self.conn.clone();
        redis::cmd("PING").query_async::<String>(&mut c).await.map_err(bus_err).map(|_| ())
    }

    async fn subscribe(&self, mode: SubscriptionMode) -> AppResult<Box<dyn Subscription>> {
        let mut conn = self.client.get_multiplexed_async_connection().await.map_err(bus_err)?;
        let stream = self.cfg.stream.clone();
        match mode {
            SubscriptionMode::Group { group, consumer } => {
                // Create the group at the start of the stream (idempotent consumers make replay safe).
                let created: Result<(), RedisError> = conn.xgroup_create_mkstream(&stream, &group, "0").await;
                if let Err(e) = created {
                    if e.code() != Some("BUSYGROUP") {
                        return Err(bus_err(e));
                    }
                }
                Ok(Box::new(GroupSub {
                    conn,
                    stream,
                    group,
                    consumer,
                    buf: VecDeque::new(),
                    recovering: true,
                    reads: 0,
                    block_ms: self.cfg.block_ms,
                    claim_idle_ms: self.cfg.claim_idle_ms,
                }))
            }
            SubscriptionMode::Broadcast => {
                // Start after the newest entry: broadcast is "from now on".
                let last: StreamRangeLast = conn.xrevrange_count(&stream, "+", "-", 1).await.map_err(bus_err)?;
                Ok(Box::new(BroadcastSub { conn, stream, last_id: last.0.unwrap_or_else(|| "0-0".into()), buf: VecDeque::new(), block_ms: self.cfg.block_ms }))
            }
        }
    }
}

/// Newest entry id from an XREVRANGE … COUNT 1 reply.
struct StreamRangeLast(Option<String>);

impl redis::FromRedisValue for StreamRangeLast {
    fn from_redis_value(v: &Value) -> redis::RedisResult<Self> {
        let r: redis::streams::StreamRangeReply = redis::FromRedisValue::from_redis_value(v)?;
        Ok(Self(r.ids.into_iter().next().map(|x| x.id)))
    }
}

fn to_delivery(sid: &StreamId) -> Option<Delivery> {
    let payload: Option<String> = sid.get("payload");
    match payload.as_deref().map(EventEnvelope::from_json) {
        Some(Ok(envelope)) => Some(Delivery { id: sid.id.clone(), envelope }),
        Some(Err(e)) => {
            tracing::error!(entry = %sid.id, error = %e, "skipping undecodable bus message");
            None
        }
        None => {
            tracing::error!(entry = %sid.id, "skipping bus message without payload");
            None
        }
    }
}

struct GroupSub {
    conn: MultiplexedConnection,
    stream: String,
    group: String,
    consumer: String,
    buf: VecDeque<Delivery>,
    /// Still draining this consumer's own pending (delivered, unacked) entries.
    recovering: bool,
    reads: u32,
    block_ms: usize,
    claim_idle_ms: usize,
}

impl GroupSub {
    /// Push decodable entries into the buffer; ack poison messages so they don't loop.
    async fn take(&mut self, ids: Vec<StreamId>) -> AppResult<usize> {
        let mut n = 0;
        for sid in ids {
            match to_delivery(&sid) {
                Some(d) => {
                    self.buf.push_back(d);
                    n += 1;
                }
                None => {
                    let _: i64 = self.conn.xack(&self.stream, &self.group, &[&sid.id]).await.map_err(bus_err)?;
                }
            }
        }
        Ok(n)
    }

    async fn read(&mut self, id: &str, block: Option<usize>) -> AppResult<Vec<StreamId>> {
        let mut opts = StreamReadOptions::default().group(&self.group, &self.consumer).count(BATCH);
        if let Some(ms) = block {
            opts = opts.block(ms);
        }
        let reply: Option<StreamReadReply> = self.conn.xread_options(&[&self.stream], &[id], &opts).await.map_err(bus_err)?;
        Ok(reply.map(|r| r.keys.into_iter().flat_map(|k| k.ids).collect()).unwrap_or_default())
    }

    /// Take over entries idle on other (probably dead) consumers.
    async fn claim_stale(&mut self) -> AppResult<Vec<StreamId>> {
        let v: Value = redis::cmd("XAUTOCLAIM")
            .arg(&self.stream)
            .arg(&self.group)
            .arg(&self.consumer)
            .arg(self.claim_idle_ms)
            .arg("0-0")
            .arg("COUNT")
            .arg(BATCH)
            .query_async(&mut self.conn)
            .await
            .map_err(bus_err)?;
        // Reply: [next-cursor, [[id, [field, value, …]], …], [deleted ids…]]
        let Value::Array(parts) = v else { return Ok(vec![]) };
        let Some(entries) = parts.get(1) else { return Ok(vec![]) };
        let r: redis::streams::StreamRangeReply = redis::FromRedisValue::from_redis_value(entries).map_err(bus_err)?;
        if !r.ids.is_empty() {
            tracing::info!(count = r.ids.len(), consumer = %self.consumer, "claimed stale bus messages");
        }
        Ok(r.ids)
    }
}

#[async_trait]
impl Subscription for GroupSub {
    async fn next(&mut self) -> AppResult<Option<Delivery>> {
        loop {
            if let Some(d) = self.buf.pop_front() {
                return Ok(Some(d));
            }
            if self.recovering {
                let ids = self.read("0", None).await?;
                if ids.is_empty() {
                    self.recovering = false;
                } else {
                    tracing::info!(count = ids.len(), "re-delivering pending bus messages after restart");
                    self.take(ids).await?;
                }
                continue;
            }
            self.reads = self.reads.wrapping_add(1);
            if self.reads % CLAIM_EVERY == 1 {
                let ids = self.claim_stale().await?;
                if self.take(ids).await? > 0 {
                    continue;
                }
            }
            let ids = self.read(">", Some(self.block_ms)).await?;
            self.take(ids).await?;
        }
    }

    async fn ack(&mut self, d: &Delivery) -> AppResult<()> {
        let _: i64 = self.conn.xack(&self.stream, &self.group, &[&d.id]).await.map_err(bus_err)?;
        Ok(())
    }
}

struct BroadcastSub {
    conn: MultiplexedConnection,
    stream: String,
    last_id: String,
    buf: VecDeque<Delivery>,
    block_ms: usize,
}

#[async_trait]
impl Subscription for BroadcastSub {
    async fn next(&mut self) -> AppResult<Option<Delivery>> {
        loop {
            if let Some(d) = self.buf.pop_front() {
                return Ok(Some(d));
            }
            let opts = StreamReadOptions::default().count(BATCH).block(self.block_ms);
            let reply: Option<StreamReadReply> = self.conn.xread_options(&[&self.stream], &[&self.last_id], &opts).await.map_err(bus_err)?;
            for sid in reply.map(|r| r.keys.into_iter().flat_map(|k| k.ids).collect::<Vec<_>>()).unwrap_or_default() {
                self.last_id = sid.id.clone();
                if let Some(d) = to_delivery(&sid) {
                    self.buf.push_back(d);
                }
            }
        }
    }

    async fn ack(&mut self, _d: &Delivery) -> AppResult<()> {
        Ok(())
    }
}
