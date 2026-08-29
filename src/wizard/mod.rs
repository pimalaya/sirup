//! Wizard generating one account: it takes a single URL, email or domain
//! input, probes PACC then Autoconfig then SRV for the server endpoints,
//! prompts for the SASL credentials, and hands back a ready-to-place
//! `[accounts.<name>]` table.
//!
//! [`configure`] owns what happens to that table (created, appended or
//! printed) and is always compiled, so `sirup configure` exists whatever
//! the feature set and says what is missing when it cannot run. The
//! discovery probes need both protocols and a TLS provider, and are
//! gated on the `discovery` cfg the build script sets.

#[cfg(discovery)]
pub mod autoconfig;
pub mod configure;
#[cfg(discovery)]
pub mod discover;
#[cfg(discovery)]
pub mod pacc;
#[cfg(discovery)]
pub mod secret;
#[cfg(discovery)]
pub mod srv;
