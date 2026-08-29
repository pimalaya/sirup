//! # Sirup
//!
//! Sirup spawns a pre-authenticated IMAP or SMTP session and exposes it
//! on a Unix socket, so any local client can speak the raw protocol
//! without holding credentials or repeating the login and TLS
//! handshake. It runs as a small blocking daemon, one instance per
//! account, best placed behind a systemd service or equivalent.
//!
//! ## Layout
//!
//! [`cli`] is the parser and the dispatcher, resolving the account a
//! command runs against. [`config`] models the on-disk TOML: one account
//! per entry, each carrying a server address, a TLS profile, an optional
//! STARTTLS switch and a single SASL mechanism. [`session`] is the daemon
//! itself: it opens and authenticates the upstream connection, binds the
//! socket, replaces the protocol greeting with a PREAUTH one and proxies
//! bytes both ways, issuing a periodic NOOP to keep the upstream alive
//! while idle. [`repl`] is a minimal reference client that connects to
//! the socket and forwards raw commands, used for testing and as an
//! implementation example.
//!
//! ## Wizard
//!
//! [`wizard`] resolves an account from a single email, URL or domain
//! input: it probes PACC, Thunderbird Autoconfig and RFC 6186 SRV
//! through io-pim-discovery, prompts for the SASL credentials, tests the
//! account by connecting once, then writes it to a configuration that
//! does not exist yet, appends it to one that does, or prints it. It
//! runs from `sirup configure`, and from the offer a bare `sirup` raises
//! when it finds no configuration.
//!
//! ## Features
//!
//! Protocol support (imap, smtp) and the TLS provider (rustls-ring,
//! rustls-aws, native-tls) are cargo features. Discovery needs both
//! protocols and a TLS provider, and `sirup configure` says so when it
//! is built without them.
//!
//! The design memory lives in the cairn/ folder (the Cairn convention:
//! spec/ for current truth, changes/ for proposals, log/ for history).

mod cli;
mod config;
mod json_schema;
mod protocol;
mod repl;
mod session;
mod wizard;

use std::{
    io::{IsTerminal, stdin},
    path::PathBuf,
};

use anyhow::Result;
use clap::{CommandFactory, Parser};
use pimalaya_cli::{
    error::ErrorReport,
    log::Logger,
    printer::{Printer, StdoutPrinter},
};
use pimalaya_config::toml::TomlConfig;

use crate::{cli::Cli, config::Config};

fn main() {
    let cli = Cli::parse();
    let mut printer = StdoutPrinter::new(&cli.json);
    let result = execute(cli, &mut printer);
    ErrorReport::eval(&mut printer, result);
}

fn execute(cli: Cli, printer: &mut StdoutPrinter) -> Result<()> {
    Logger::try_init(&cli.log)?;
    let config_paths = cli.config.paths.as_ref();
    let account_name = cli.account.name.as_deref();

    let Some(cmd) = cli.cmd else {
        return meet_bare_invocation(printer, config_paths, account_name.is_some());
    };

    cmd.execute(printer, config_paths, account_name)
}

/// Meets a bare `sirup`, which is where a newcomer lands.
///
/// A missing configuration raises the offer; anything else gets the help.
/// A broken file counts as a configuration, so the offer never writes over
/// one, and `--account` alone is a half-typed command, not a first run.
fn meet_bare_invocation(
    printer: &mut StdoutPrinter,
    config_paths: &[PathBuf],
    named_account: bool,
) -> Result<()> {
    let configured = Config::from_paths_or_default(config_paths)
        .ok()
        .flatten()
        .is_some();

    if !configured && !named_account && !printer.is_json() && stdin().is_terminal() {
        let path = Config::target_path(config_paths)?;

        // NOTE: a bare invocation has nothing to run after the offer, so a
        // declined one falls back to the help.
        if cli::offer_configuration(printer, config_paths, &path)? {
            return Ok(());
        }
    }

    Cli::command().print_help()?;

    Ok(())
}
