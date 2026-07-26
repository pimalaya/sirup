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
//! [`config`] models the on-disk TOML: one account per entry, each
//! carrying a server address, a TLS profile, an optional STARTTLS switch
//! and a single SASL mechanism. [`session`] is the daemon itself: it
//! opens and authenticates the upstream connection, binds the socket,
//! replaces the protocol greeting with a PREAUTH one and proxies bytes
//! both ways, issuing a periodic NOOP to keep the upstream alive while
//! idle. [`repl`] is a minimal reference client that connects to the
//! socket and forwards raw commands, used for testing and as an
//! implementation example.
//!
//! ## Wizard
//!
//! Bare `sirup` (no subcommand) runs the wizard: from a single email,
//! URL or domain input it probes PACC, Thunderbird Autoconfig and RFC
//! 6186 SRV through io-pim-discovery, prompts for the SASL credentials,
//! then prints the account as a ready-to-save `[accounts.<name>]` TOML
//! fragment on stdout. It starts no daemon and writes nothing to disk;
//! redirect the output into your config (`sirup >> <config>`).
//!
//! ## Features
//!
//! Protocol support (imap, smtp) and the TLS provider (rustls-ring,
//! rustls-aws, native-tls) are cargo features. The wizard needs both
//! protocols and a TLS provider to be enabled.
//!
//! The design memory lives in the cairn/ folder (the Cairn convention:
//! spec/ for current truth, changes/ for proposals, log/ for history).

mod config;
mod repl;
mod session;
#[cfg(all(feature = "imap", feature = "smtp"))]
#[cfg(any(
    feature = "rustls-ring",
    feature = "rustls-aws",
    feature = "native-tls"
))]
mod wizard;

use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
use pimalaya_cli::{
    clap::{
        args::{AccountFlag, JsonFlag, LogFlags},
        commands::{CompletionCommand, ManualCommand},
        parsers::path_parser,
    },
    error::ErrorReport,
    log::Logger,
    long_version,
    printer::{Printer, StdoutPrinter},
};
use pimalaya_config::toml::TomlConfig;
use pimalaya_stream::{sasl::Sasl, tls::Tls};
use url::Url;

use crate::config::{AccountConfig, Config, parse_server};

fn main() {
    let cli = Cli::parse();
    let mut printer = StdoutPrinter::new(&cli.json);
    let result = execute(cli, &mut printer);
    ErrorReport::eval(&mut printer, result);
}

fn execute(cli: Cli, printer: &mut StdoutPrinter) -> Result<()> {
    Logger::try_init(&cli.log)?;
    let config_paths = cli.config_paths.as_ref();
    let account_name = cli.account.name.as_deref();

    match cli.cmd {
        Some(cmd) => cmd.execute(printer, config_paths, account_name),
        None => wizard_run(printer),
    }
}

#[derive(Debug, Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(author, version, about)]
#[command(long_version = long_version!())]
#[command(propagate_version = true, infer_subcommands = true)]
pub struct Cli {
    /// The subcommand to run; bare `sirup` (no subcommand) runs the
    /// wizard and prints an account config fragment on stdout.
    #[command(subcommand)]
    pub cmd: Option<Command>,
    /// Override the default configuration file path.
    ///
    /// The given paths are shell-expanded then canonicalized (if
    /// applicable). Other paths are merged with the first one, which
    /// allows you to separate your public config from your private
    /// one(s).
    /// you can also provide multiple paths by delimiting them with a :
    /// like you would when setting $PATH in a posix shell
    #[arg(short, long = "config", global = true, env = "SIRUP_CONFIG")]
    #[arg(value_name = "PATH", value_parser = path_parser, value_delimiter = ':')]
    pub config_paths: Vec<PathBuf>,
    /// Name of the account to run the command with.
    #[command(flatten)]
    pub account: AccountFlag,
    /// Switch the output format to JSON.
    #[command(flatten)]
    pub json: JsonFlag,
    /// Log level and log file destination.
    #[command(flatten)]
    pub log: LogFlags,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start a pre-authenticated IMAP/SMTP session for the given account,
    /// proxied to a Unix socket.
    ///
    /// The protocol is selected from the account's server scheme
    /// (`imap`/`imaps` or `smtp`/`smtps`). This command runs as a blocking
    /// daemon, best placed inside a systemd service or equivalent.
    Start,
    /// Start a basic REPL against the pre-authenticated session for the given
    /// account.
    ///
    /// The REPL connects to the Unix socket spawned by `start` and forwards raw
    /// IMAP or SMTP commands (picked from the account's server scheme). Mostly
    /// intended for testing: it confirms the account is properly configured and
    /// that the socket-backed session is reachable.
    Repl,
    Manuals(ManualCommand),
    Completions(CompletionCommand),
}

impl Command {
    pub fn execute(
        self,
        printer: &mut impl Printer,
        config_paths: &[PathBuf],
        account_name: Option<&str>,
    ) -> Result<()> {
        match self {
            Command::Start => {
                let (default_sock_path, mut account_config) =
                    take_account(config_paths, account_name)?;
                let sock_path = account_config.sock_file.take().unwrap_or(default_sock_path);
                let (server, tls, starttls, sasl) = resolve_connection(&account_config)?;
                session::start(sock_path, server, tls, starttls, sasl)
            }

            Command::Repl => {
                let (default_sock_path, account_config) = take_account(config_paths, account_name)?;
                let sock_path = account_config.sock_file.unwrap_or(default_sock_path);
                let server = parse_server(&account_config.server)?;
                repl::start(sock_path, server)
            }

            Command::Manuals(cmd) => cmd.execute(printer, Cli::command()),
            Command::Completions(cmd) => cmd.execute(printer, Cli::command()),
        }
    }
}

/// Resolves the per-account sock path + [`AccountConfig`] pair for the
/// requested account from the on-disk config: the account named by `-a`,
/// or the `default = true` one when none is given. A missing config file
/// or an unknown account is a hard error, with no wizard fallback (bare
/// `sirup` runs the wizard instead).
fn take_account(config_paths: &[PathBuf], name: Option<&str>) -> Result<(PathBuf, AccountConfig)> {
    let Some(mut config) = Config::from_paths_or_default(config_paths)? else {
        bail!("Cannot find configuration file; run `sirup` to generate one");
    };

    let Some((account_name, account_config)) = config.take_account(name)? else {
        bail!("Cannot find account");
    };

    let sock_path = config.sock_path(&account_name);
    Ok((sock_path, account_config))
}

/// Resolves the runtime connection parameters (server URL, TLS handle,
/// STARTTLS switch and SASL) from an account config, inferring a default
/// ALPN and port from the server scheme when unset. Shared by `start`
/// and the wizard's account test.
fn resolve_connection(account: &AccountConfig) -> Result<(Url, Tls, bool, Option<Sasl>)> {
    let server = parse_server(&account.server)?;
    // NOTE: a missing alpn infers from the server scheme (["imap"] for
    // imap[s]://, ["smtp"] for smtp[s]://); an explicit empty vec
    // disables ALPN, a non-empty one overrides the default.
    let alpn = account
        .alpn
        .clone()
        .unwrap_or_else(|| match server.scheme() {
            #[cfg(feature = "imap")]
            "imap" | "imaps" => io_imap::client::default_alpn(),
            #[cfg(feature = "smtp")]
            "smtp" | "smtps" => io_smtp::client::SmtpClientStd::default_alpn(),
            _ => Vec::new(),
        });
    let tls = account.tls.clone().into_tls(alpn);
    let starttls = account.starttls;
    // NOTE: the url crate only knows default ports for web schemes, so
    // imap(s)/smtp(s) need an explicit fallback. Gating the SASL config
    // on port_or_known_default() would silently drop it for a portless
    // URL like `imaps://mail.example.com`, opening an unauthenticated
    // session. Host and port only feed OAUTHBEARER; the other mechanisms
    // ignore them.
    let sasl = account
        .sasl
        .clone()
        .map(|cfg| {
            let host = server.host_str().unwrap_or_default();
            let scheme = server.scheme();
            let port = server.port().unwrap_or_else(|| match scheme {
                #[cfg(feature = "imap")]
                "imap" | "imaps" => io_imap::client::default_port(scheme),
                #[cfg(feature = "smtp")]
                "smtp" | "smtps" => io_smtp::client::SmtpClientStd::default_port(scheme),
                _ => 0,
            });
            cfg.try_into_sasl(host, port)
        })
        .transpose()?;

    Ok((server, tls, starttls, sasl))
}

/// Validates an account by opening and authenticating the upstream
/// session once, then dropping it. Used by the wizard to test a
/// freshly-built account before printing it, exactly like himalaya.
#[cfg(all(feature = "imap", feature = "smtp"))]
#[cfg(any(
    feature = "rustls-ring",
    feature = "rustls-aws",
    feature = "native-tls"
))]
pub(crate) fn test_account(account: &AccountConfig) -> Result<()> {
    let (server, tls, starttls, sasl) = resolve_connection(account)?;
    session::test(server, tls, starttls, sasl)
}

#[cfg(all(
    feature = "imap",
    feature = "smtp",
    any(
        feature = "rustls-ring",
        feature = "rustls-aws",
        feature = "native-tls"
    )
))]
fn wizard_run(printer: &mut impl Printer) -> Result<()> {
    wizard::discover::run(printer)
}

#[cfg(not(all(
    feature = "imap",
    feature = "smtp",
    any(
        feature = "rustls-ring",
        feature = "rustls-aws",
        feature = "native-tls"
    )
)))]
fn wizard_run(_printer: &mut impl Printer) -> Result<()> {
    bail!(
        "The wizard requires the `imap`, `smtp` and a TLS feature \
         (`rustls-ring`, `rustls-aws` or `native-tls`); pass a subcommand \
         to run against an existing configuration file instead"
    )
}
