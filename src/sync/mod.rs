//! FreshRSS sync support (GReader API).
//!
//! Layout:
//! - `secret`: credential storage — OS keyring when available, else a `0600`
//!   file in the data dir, with `BROCHURE_FRESHRSS_PASSWORD` env override.
//! - `freshrss`: GReader API response types + pure mapping helpers (no I/O).
//! - `client`: async HTTP calls built on the shared [`crate::fetch`] client.

pub mod client;
pub mod freshrss;
pub mod secret;
