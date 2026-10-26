//! # feast-domain
//!
//! The innermost layer: the contract types every client speaks, the domain events
//! the two sides of the marketplace exchange, and pure business rules (pricing,
//! slots). It has no I/O and no framework dependencies, so it can be unit-tested
//! in isolation and ported to another language rule-for-rule (see docs/BUSINESS_RULES.md).

pub mod arrival;
pub mod audit;
pub mod compliance;
pub mod deals;
pub mod events;
pub mod insights;
pub mod listing;
pub mod market;
pub mod media;
pub mod model;
pub mod offline;
pub mod pricing;
pub mod slots;
pub mod subscription;
pub mod trust;

pub use events::{DomainEvent, EventEnvelope, Party};
