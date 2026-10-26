//! The notification service.
//!
//! Owns notification storage (its own database) and turns "notify these users
//! about this event" into stored, idempotent notifications, then announces each
//! one on the bus as `notification.created` so gateways can push it live.
//!
//! Deployment options (config `notifications.mode`):
//! - **embedded** — the core calls [`NotificationService`] in-process (it implements
//!   the `NotificationGateway` port directly). Simplest; one binary.
//! - **grpc** — run `feast notification` as its own process; the core uses
//!   [`client::GrpcNotificationGateway`]. Scale and deploy it independently.

// gRPC handlers return tonic::Status (large by design).
#![allow(clippy::result_large_err)]

pub mod client;
pub mod convert;
pub mod server;
pub mod service;
pub mod store;

pub use client::GrpcNotificationGateway;
pub use server::{serve, NotificationGrpc};
pub use service::NotificationService;
pub use store::{NotificationStore, SqlNotificationStore};
