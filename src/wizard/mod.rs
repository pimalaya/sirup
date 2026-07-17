//! In-memory, first-run wizard: takes one URL/email/domain input,
//! probes PACC → Autoconfig → SRV for the server endpoints, then
//! prompts for SASL credentials. Returns a single in-memory
//! [`crate::config::AccountConfig`]; no on-disk config is written.

pub mod autoconfig;
pub mod discover;
pub mod pacc;
pub mod srv;
