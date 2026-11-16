//! # Sirup
//!
//! Sirup spawns a pre-authenticated IMAP, SMTP or ManageSieve session
//! and exposes it on a Unix socket, so any local client can speak the
//! raw protocol without holding credentials or repeating the login and
//! TLS handshake. It runs as a small blocking daemon, one instance per
//! account serving one socket per protocol that account declares, best
//! placed behind a systemd service or equivalent.
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
//! ## First run
//!
//! Sirup writes no configuration: an account is written by hand from
//! config.sample.toml. A person at a terminal missing one meets
//! pimalaya-cli's welcome, naming the commands creating it from the
//! sample, a script or a JSON caller getting the failure alone.
//!
//! ## Features
//!
//! Protocol support (imap, smtp, sieve) and the TLS provider
//! (rustls-ring, rustls-aws, native-tls) are cargo features.
//!
//! The design memory lives in the cairn/ folder (the Cairn convention:
//! spec/ for current truth, changes/ for proposals, log/ for history).

mod cli;
mod config;
mod json_schema;
mod protocol;
mod repl;
mod session;

use std::{path::PathBuf, process};

use anyhow::Result;
use pimalaya_cli::{
    clap::{args::LogFlags, parse},
    error::{EXIT_FAILURE, ErrorReport},
    log::Logger,
    printer::StdoutPrinter,
    welcome::{is_wanted, shipped_sample},
};
use pimalaya_config::toml::ConfigMissingError;

use crate::cli::{Cli, Command};

fn main() {
    let cli = parse::<Cli>();
    let mut printer = StdoutPrinter::new(&cli.json);
    let config_paths = &cli.config.paths;
    let account_name = cli.account.name.as_deref();

    let result = execute(&mut printer, cli.cmd, &cli.log, config_paths, account_name);

    // A person missing a configuration meets the welcome, a script or a
    // JSON caller the error.
    if let Err(err) = &result
        && let Some(missing) = err.downcast_ref::<ConfigMissingError>()
        && is_wanted(&printer)
    {
        let sample = shipped_sample("sirup");
        let welcome = cli::welcome(&missing.path, config_paths, sample.as_deref());
        eprint!("{welcome}");
        process::exit(EXIT_FAILURE);
    }

    ErrorReport::eval(&mut printer, result);
}

/// Runs the parsed command, or meets a bare `sirup`.
fn execute(
    printer: &mut StdoutPrinter,
    cmd: Option<Command>,
    log: &LogFlags,
    config_paths: &[PathBuf],
    account_name: Option<&str>,
) -> Result<()> {
    Logger::try_init(log)?;

    let Some(cmd) = cmd else {
        return cli::meet_bare_invocation(config_paths);
    };

    cmd.execute(printer, config_paths, account_name)
}
