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
//! When no configuration file is found, the wizard module builds an
//! account in memory from a single email, URL or domain input: it
//! probes PACC, Thunderbird Autoconfig and RFC 6186 SRV through
//! io-pim-discovery, then prompts for the SASL credentials. Nothing is
//! written to disk.
//!
//! ## Features
//!
//! Protocol support (imap, smtp) and the TLS provider (rustls-ring,
//! rustls-aws, native-tls) are cargo features; the wizard needs both
//! protocols and a TLS provider to be enabled.

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
    cli.cmd.execute(printer, config_paths, cli.no_account)
}

#[derive(Debug, Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
#[command(author, version, about)]
#[command(long_version = long_version!())]
#[command(propagate_version = true, infer_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
    /// Override the default configuration file path.
    ///
    /// The given paths are shell-expanded then canonicalized (if
    /// applicable). If the first path does not point to a valid file,
    /// the wizard will propose to assist you in the creation of the
    /// configuration file. Other paths are merged with the first one,
    /// which allows you to separate your public config from your
    /// private(s) one(s).
    /// you can also provide multiple paths by delimiting them with a :
    /// like you would when setting $PATH in a posix shell
    #[arg(short, long = "config", global = true, env = "SIRUP_CONFIG")]
    #[arg(value_name = "PATH", value_parser = path_parser, value_delimiter = ':')]
    pub config_paths: Vec<PathBuf>,
    /// Skip configuration file lookup and run the wizard.
    ///
    /// Useful when a config already exists on disk but you want a
    /// throwaway, in-memory account for this run (e.g. to try another
    /// server, or hand off the daemon to someone else without exposing
    /// your stored credentials). The wizard never writes to disk;
    /// `--config` and `SIRUP_CONFIG` are ignored when this flag is set.
    #[arg(long = "no-account", global = true)]
    pub no_account: bool,
    #[command(flatten)]
    pub json: JsonFlag,
    #[command(flatten)]
    pub log: LogFlags,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start a pre-authenticated IMAP/SMTP session for the given account,
    /// proxied to a Unix socket.
    ///
    /// The protocol is selected from the account's server scheme (`imap`/`imaps`
    /// or `smtp`/`smtps`). This command runs as a blocking daemon; best place
    /// is inside a systemd service or equivalent.
    Start {
        #[command(flatten)]
        account: AccountFlag,
    },
    /// Start a basic REPL against the pre-authenticated session for the given
    /// account.
    ///
    /// The REPL connects to the Unix socket spawned by `start` and forwards raw
    /// IMAP or SMTP commands (picked from the account's server scheme). Mostly
    /// intended for testing: it confirms the account is properly configured and
    /// that the socket-backed session is reachable.
    Repl {
        #[command(flatten)]
        account: AccountFlag,
    },
    Manuals(ManualCommand),
    Completions(CompletionCommand),
}

impl Command {
    pub fn execute(
        self,
        printer: &mut impl Printer,
        config_paths: &[PathBuf],
        no_account: bool,
    ) -> Result<()> {
        match self {
            Command::Start { account } => {
                let (default_sock_path, mut account_config) =
                    load_or_wizard(config_paths, account.name.as_deref(), no_account)?;
                let sock_path = account_config.sock_file.take().unwrap_or(default_sock_path);
                let server = parse_server(&account_config.server)?;
                // NOTE: a missing alpn infers from the server scheme (["imap"]
                // for imap[s]://, ["smtp"] for smtp[s]://); an explicit empty
                // vec disables ALPN, a non-empty one overrides the default.
                let alpn = account_config
                    .alpn
                    .unwrap_or_else(|| match server.scheme() {
                        #[cfg(feature = "imap")]
                        "imap" | "imaps" => io_imap::client::default_alpn(),
                        #[cfg(feature = "smtp")]
                        "smtp" | "smtps" => io_smtp::client::SmtpClientStd::default_alpn(),
                        _ => Vec::new(),
                    });
                let tls = account_config.tls.into_tls(alpn);
                let starttls = account_config.starttls;
                // NOTE: the url crate only knows default ports for web
                // schemes, so imap(s)/smtp(s) need an explicit fallback.
                // Gating the SASL config on port_or_known_default() would
                // silently drop it for a portless URL like
                // `imaps://mail.example.com`, opening an unauthenticated
                // session. Host and port only feed OAUTHBEARER; the other
                // mechanisms ignore them.
                let sasl = account_config
                    .sasl
                    .map(|cfg| {
                        let host = server.host_str().unwrap_or_default();
                        let scheme = server.scheme();
                        let port = server.port().unwrap_or_else(|| match scheme {
                            #[cfg(feature = "imap")]
                            "imap" | "imaps" => io_imap::client::default_port(scheme),
                            #[cfg(feature = "smtp")]
                            "smtp" | "smtps" => {
                                io_smtp::client::SmtpClientStd::default_port(scheme)
                            }
                            _ => 0,
                        });
                        cfg.try_into_sasl(host, port)
                    })
                    .transpose()?;

                session::start(sock_path, server, tls, starttls, sasl)
            }

            Command::Repl { account } => {
                let (default_sock_path, account_config) =
                    load_or_wizard(config_paths, account.name.as_deref(), no_account)?;
                let sock_path = account_config.sock_file.unwrap_or(default_sock_path);
                let server = parse_server(&account_config.server)?;

                repl::start(sock_path, server)
            }

            Command::Manuals(cmd) => cmd.execute(printer, Cli::command()),
            Command::Completions(cmd) => cmd.execute(printer, Cli::command()),
        }
    }
}

/// Resolves the per-account sock path + AccountConfig pair for the
/// requested account: loads it from the on-disk config when available,
/// otherwise runs the interactive wizard to build one in memory. The
/// wizard never writes to disk. `no_account` short-circuits the config
/// lookup and forces the wizard.
fn load_or_wizard(
    config_paths: &[PathBuf],
    name: Option<&str>,
    no_account: bool,
) -> Result<(PathBuf, AccountConfig)> {
    if !no_account {
        if let Some(mut config) = Config::from_paths_or_default(config_paths)? {
            let Some((account_name, account_config)) = config.take_account(name)? else {
                bail!("Cannot find account");
            };
            let sock_path = config.sock_path(&account_name);
            return Ok((sock_path, account_config));
        }
    }

    let account_config = wizard_run()?;
    let account_name = name.unwrap_or("wizard");
    let sock_path = Config::default_for_wizard().sock_path(account_name);
    Ok((sock_path, account_config))
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
fn wizard_run() -> Result<AccountConfig> {
    wizard::discover::run()
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
fn wizard_run() -> Result<AccountConfig> {
    bail!(
        "No config file found, and the wizard requires the `imap`, `smtp` \
         and a TLS feature (`rustls-ring`, `rustls-aws` or `native-tls`)"
    )
}
