//! Wizard run on bare `sirup`: takes one URL/email/domain input,
//! probes PACC → Autoconfig → SRV for the server endpoints, then
//! prompts for SASL credentials. Prints the resulting
//! [`crate::config::AccountConfig`] as a ready-to-save TOML fragment on
//! stdout; no on-disk config is written.

pub mod autoconfig;
pub mod discover;
pub mod pacc;
pub mod secret;
pub mod srv;
