//! Infrastructure adapters for the Feast ports.
//!
//! Every module here implements one or more traits from `feast_application::ports`.
//! Nothing in `domain` or `application` depends on this crate — the server
//! binary picks adapters from config and wires them together (composition root).
//!
//! | Port                     | Adapters                                   |
//! |--------------------------|--------------------------------------------|
//! | repositories + outbox    | [`sqlite::SqlStore`] (via [`database`]) |
//! | `MessageBus`             | [`bus::memory`], [`bus::redis`]            |
//! | `TokenService`           | [`auth::JwtTokenService`]                  |
//! | `PasswordHasher`         | [`auth::Argon2Hasher`]                     |
//! | `PaymentGateway`         | [`payments`]: mock, Razorpay, Stripe (registry + factories) |
//! | `FileStorage`            | [`storage`]: local disk, S3-compatible     |
//! | `GeoLocator`             | [`geo`]: CDN headers, MaxMind              |
//! | `MarketRepository` etc.  | [`sqlite::SqlStore`]                    |
//! | `OtpService`             | [`otp::FixedOtpService`], [`otp::LogOtpService`] |
//! | `IdentityProvider`       | [`oauth`]: Google/Apple (OIDC), Facebook, demo |
//! | `Clock`                  | [`clock::SystemClock`], [`clock::FixedClock`] |
//! | `IdGenerator`            | [`ids::UuidIds`]                           |
//! | `DemoDataPort`           | [`seed::DemoReset`]                        |

pub mod auth;
pub mod bus;
pub mod clock;
pub mod database;
pub mod geo;
pub mod ids;
pub mod media;
pub mod oauth;
pub mod otp;
pub mod payments;
pub mod push;
pub mod seed;
#[cfg(feature = "sql")]
pub mod sql;
pub mod storage;
