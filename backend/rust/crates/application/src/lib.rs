//! # feast-application
//!
//! Use cases and ports (hexagonal architecture). This crate defines *what* the
//! system does and the interfaces (`ports`) it needs from the outside world; it
//! knows nothing about HTTP, SQL, Redis or gRPC. Infrastructure crates implement
//! the ports; delivery crates (HTTP gateway, gRPC services, workers) call the
//! use cases on [`Feast`].

pub mod actor;
pub mod admin;
pub mod audit_trail;
pub mod auth;
pub mod billing;
pub mod booking;
pub mod compliance;
pub mod context;
pub mod discovery;
pub mod error;
pub mod insights;
pub mod markets;
pub mod media;
pub mod notifications;
pub mod offline;
pub mod partner;
pub mod ports;
pub mod processes;
pub mod qr;
pub mod subscriptions;
pub mod trust;

pub use context::{AppSettings, Feast, Ports};
pub use error::{AppError, AppResult, ErrorKind};
