//! FreshRSS sync support (GReader API).
//!
//! Layout:
//! - `secret`: credential storage — OS keyring when available, else a `0600`
//!   file in the data dir, with `BROCHURE_FRESHRSS_PASSWORD` env override.
//! - `freshrss`: GReader API response types + pure mapping helpers (no I/O).

pub mod freshrss;
pub mod secret;

/// Common behaviour for a remote aggregator (FreshRSS today, others later).
pub trait SyncProvider {
    /// Pull subscriptions + articles; returns number of articles merged.
    fn name(&self) -> &'static str;
}
